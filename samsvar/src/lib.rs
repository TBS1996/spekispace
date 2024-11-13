pub use serde_json::{json, Value};

mod parser {
    use super::{Expr, Schema};
    use nom::{
        branch::alt,
        character::complete::{char, multispace0},
        multi::separated_list0,
        sequence::{delimited, pair, preceded},
        IResult,
    };

    // Top-level parser for logical expressions (AND/OR combined)
    pub fn evaluate(input: &str) -> IResult<&str, Schema> {
        alt((or_expr, and_expr, parens, condition))(input)
    }

    fn condition(input: &str) -> IResult<&str, Schema> {
        let (input, cond) = nom::bytes::complete::take_while(|c: char| {
            c.is_alphanumeric() || c.is_whitespace() || "><=!.".contains(c)
        })(input)?;
        Ok((
            input,
            Schema::Expr(Expr::from_string(cond.trim().to_string()).unwrap()),
        ))
    }

    fn parens(input: &str) -> IResult<&str, Schema> {
        delimited(
            pair(char('('), multispace0),
            evaluate,
            pair(multispace0, char(')')),
        )(input)
    }

    fn and_expr(input: &str) -> IResult<&str, Schema> {
        let (input, exprs) = separated_list0(
            preceded(multispace0, char('&')),
            preceded(multispace0, alt((parens, condition))),
        )(input)?;
        Ok((input, Schema::And(exprs)))
    }

    fn or_expr(input: &str) -> IResult<&str, Schema> {
        let (input, exprs) = separated_list0(
            preceded(multispace0, char('|')),
            preceded(multispace0, alt((parens, and_expr))),
        )(input)?;
        Ok((input, Schema::Or(exprs)))
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Operand {
    GreaterThan,
    Equal,
    LessThan,
    GreaterOrEqual,
    LessOrEqual,
    Contains,
}

impl Operand {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            ">" => Some(Self::GreaterThan),
            "==" => Some(Self::Equal),
            "<" => Some(Self::LessThan),
            ">=" => Some(Self::GreaterOrEqual),
            "<=" => Some(Self::LessOrEqual),
            "contains" => Some(Self::Contains),
            _ => None,
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct Expr {
    key: String,
    op: Operand,
    val: Value,
    // if set, won't panic if key is missing but instead provide the default value
    default: Option<bool>,
    /// Should be set if the [`Value`] is a serde object, it will then get the value with this key
    dict_key: Option<String>,
}

impl Expr {
    fn new(key: &str, op: &str, val: &str) -> Self {
        Self {
            key: key.trim().to_string(),
            op: Operand::from_str(op).unwrap(),
            val: string_to_serde_value(val.trim()),
            default: None,
            dict_key: None,
        }
    }

    fn from_string(s: String) -> Option<Self> {
        let s = s.trim();

        for op in ["==", "<", ">", "<=", ">=", "contains"] {
            if let Some((key, val)) = s.split_once(op) {
                return Some(Self::new(key, op, val));
            }
        }

        None
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum Schema {
    Expr(Expr),
    And(Vec<Schema>),
    Or(Vec<Schema>),
}

impl Schema {
    fn new(s: String) -> Option<Self> {
        let mut schema = parser::evaluate(&s).unwrap().1;
        schema.simplify();
        Some(schema)
    }

    fn simplify(&mut self) {
        let inner = match self {
            Schema::Expr(_) => return,
            Schema::And(s) | Schema::Or(s) => {
                if s.len() == 1 {
                    Some(s.remove(0))
                } else {
                    for x in s.iter_mut() {
                        x.simplify();
                    }
                    None
                }
            }
        };

        if let Some(i) = inner {
            *self = i;
        }
    }
}

fn string_to_serde_value(input: &str) -> Value {
    if let Ok(boolean_value) = input.parse::<bool>() {
        return Value::Bool(boolean_value);
    }

    if let Ok(number_value) = input.parse::<f64>() {
        return Value::Number(serde_json::Number::from_f64(number_value).unwrap());
    }

    Value::String(input.to_string())
}

use async_trait::async_trait;

#[async_trait(?Send)]
pub trait Matcher: std::fmt::Debug {
    async fn get_val(&self, key: &str) -> Option<Value>;

    async fn eval(&self, s: String) -> bool {
        let schema = Schema::new(s).unwrap();
        self.eval_schema(&schema).await
    }

    async fn eval_schema(&self, schema: &Schema) -> bool {
        let mut stack = vec![(schema, true)];

        while let Some((schema, should_continue)) = stack.pop() {
            match schema {
                Schema::And(exps) => {
                    if should_continue {
                        for exp in exps.iter().rev() {
                            stack.push((exp, true));
                        }
                    }
                }
                Schema::Or(exps) => {
                    if !should_continue {
                        for exp in exps.iter().rev() {
                            stack.push((exp, false));
                        }
                    }
                }
                Schema::Expr(exp) => {
                    let is_match = self.is_match(exp).await;
                    if should_continue && !is_match {
                        return false; // fail fast for AND
                    }
                    if !should_continue && is_match {
                        return true; // succeed fast for OR
                    }
                }
                _ => {}
            }
        }

        true
    }

    async fn is_match(&self, x: &Expr) -> bool {
        let op = x.op;
        let val = x.val.to_owned();
        let arg = self.get_val(&x.key).await.unwrap().into();

        use Operand as OP;
        use Value::*;

        match (arg, op, val) {
            (Number(val), OP::GreaterOrEqual, Number(arg)) => val.as_f64() >= arg.as_f64(),
            (Number(val), OP::LessOrEqual, Number(arg)) => val.as_f64() <= arg.as_f64(),
            (Number(val), OP::GreaterThan, Number(arg)) => val.as_f64() > arg.as_f64(),
            (Number(val), OP::LessThan, Number(arg)) => val.as_f64() < arg.as_f64(),
            (String(val), OP::Contains, String(arg)) => val.contains(&arg),
            (String(val), OP::Equal, String(arg)) => val == arg,
            (Number(val), OP::Equal, Number(arg)) => val.as_f64() == arg.as_f64(),
            (Bool(val), OP::Equal, Bool(arg)) => val == arg,
            (val, op, arg) => {
                panic!(
                    "incorrect combination of operands and operator: {} {:?} {}",
                    val, op, arg
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct TestStruct {}

    use serde_json::json;

    impl Matcher for TestStruct {
        async fn get_val(&self, key: &str) -> Option<Value> {
            match key {
                "age" => Some(json!(5.)),
                "height" => Some(json!(180)),
                "msg" => Some(json!("hello world")),
                _ => None,
            }
        }
    }

    #[tokio::test]
    async fn loltest() {
        let x = TestStruct {};
        let input = "height > 170 && msg contains worl".to_string();
        let res = x.eval(input).await;
        dbg!(res);
        panic!();
    }

    #[tokio::test]
    async fn test1() {
        let input = "a>4".to_string();
        assert_eq!(
            Schema::new(input).unwrap(),
            Schema::Expr(Expr::new("a", ">", "4"))
        );
    }

    #[test]
    fn test2() {
        let input = "a < 3".to_string();
        assert_eq!(
            Schema::new(input).unwrap(),
            Schema::Expr(Expr::new("a", "<", "3"))
        );
    }

    #[test]
    fn test3() {
        let input = "a contains helloworld & b < 4 | c == true".to_string();
        assert_eq!(
            Schema::new(input).unwrap(),
            Schema::Or(vec![
                Schema::And(vec![
                    Schema::Expr(Expr::new("a", "contains", "helloworld")),
                    Schema::Expr(Expr::new("b", "<", "4"))
                ]),
                Schema::Expr(Expr::new("c", "==", "true"))
            ])
        );
    }

    #[test]
    fn test4() {
        let input = "a contains helloworld & (b < 4 | c == true)".to_string();
        assert_eq!(
            Schema::new(input).unwrap(),
            Schema::And(vec![
                Schema::Expr(Expr::new("a", "contains", "helloworld")),
                Schema::Or(vec![
                    Schema::Expr(Expr::new("b", "<", "4")),
                    Schema::Expr(Expr::new("c", "==", "true"))
                ]),
            ])
        );
    }

    #[test]
    fn test5() {
        let input = "a contains heyworld".to_string();
        assert_eq!(
            Schema::new(input).unwrap(),
            Schema::Expr(Expr::new("a", "contains", "heyworld"))
        );
    }

    #[test]
    fn tokentest() {
        let input = "a contains helloworld & (b < 4 | c == true)".to_string();
        let result = Schema::new(input);
        dbg!(result);
        panic!();
    }
}
