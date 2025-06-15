use std::{collections::BTreeSet, fmt::Display};

use ledgerstore::{LedgerItem, TheLedgerEvent};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumDiscriminants, EnumIter, EnumString};
use uuid::Uuid;

use crate::{
    card_provider::CardProvider,
    collection::{DynCard, MaybeCard},
};

impl Display for Set {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.name)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Hash)]
pub enum SetAction {
    SetName(String),
    SetExpr(SetExpr),
}

pub type SetEvent = TheLedgerEvent<Set>;

impl LedgerItem for Set {
    type Error = ();
    type Key = SetId;

    type Modifier = SetAction;
    type RefType = String;

    type PropertyType = String;

    fn inner_run_event(mut self, event: SetAction) -> Result<Self, Self::Error> {
        match event {
            SetAction::SetName(name) => self.name = name,
            SetAction::SetExpr(expr) => self.expr = expr,
        }

        Ok(self)
    }

    fn new_default(id: Self::Key) -> Self {
        Self {
            id,
            name: "...".to_string(),
            expr: SetExpr::Union(Default::default()),
        }
    }

    fn item_id(&self) -> Self::Key {
        self.id
    }
}

pub type SetId = Uuid;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Hash, Eq)]
pub struct Set {
    pub id: SetId,
    pub name: String,
    pub expr: SetExpr,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Hash, Ord, PartialOrd, Eq)]
pub enum Input {
    Leaf(DynCard),
    Reference(SetId),
    Expr(Box<SetExpr>),
}

#[derive(
    Serialize, Deserialize, Debug, Clone, PartialEq, Hash, Eq, PartialOrd, Ord, EnumDiscriminants,
)]
#[strum_discriminants(derive(
    EnumIter,
    EnumString,
    Display,
    Serialize,
    Deserialize,
    PartialOrd,
    Ord
))]
pub enum SetExpr {
    Union(BTreeSet<Input>),
    Intersection(BTreeSet<Input>),
    Difference(Input, Input),
    Complement(Input),
    All,
}

impl Default for SetExpr {
    fn default() -> Self {
        Self::Union(Default::default())
    }
}

impl Input {
    pub fn eval(&self, provider: &CardProvider) -> BTreeSet<MaybeCard> {
        match self {
            Input::Leaf(dc) => dc.evaluate(provider.clone()).into_iter().collect(),
            Input::Reference(id) => provider
                .providers
                .sets
                .load(*id)
                .unwrap()
                .expr
                .eval(provider),
            Input::Expr(expr) => expr.eval(provider),
        }
    }
}

impl SetExpr {
    pub fn universe() -> Self {
        Self::All
    }

    pub fn union_with(dyns: impl IntoIterator<Item = DynCard>) -> Self {
        let leafs: BTreeSet<Input> = dyns.into_iter().map(Input::Leaf).collect();
        Self::Union(leafs)
    }

    pub fn set_name(&self) -> &'static str {
        match self {
            SetExpr::Union(_) => "union",
            SetExpr::Intersection(_) => "intersection",
            SetExpr::Difference(_, _) => "difference",
            SetExpr::Complement(_) => "complement",
            SetExpr::All => "all",
        }
    }

    pub fn inputs(&self) -> Vec<&Input> {
        let mut out = vec![];
        match self {
            SetExpr::Union(set) => out.extend(set),
            SetExpr::Intersection(set) => out.extend(set),
            SetExpr::Difference(input, input1) => {
                out.push(input);
                out.push(input1);
            }
            SetExpr::Complement(input) => out.push(input),
            SetExpr::All => {}
        }
        out
    }

    pub fn eval(&self, provider: &CardProvider) -> BTreeSet<MaybeCard> {
        match self {
            SetExpr::Union(hash_set) => {
                let mut out: BTreeSet<MaybeCard> = Default::default();
                for input in hash_set {
                    out.extend(input.eval(provider));
                }
                out
            }
            SetExpr::Intersection(hash_set) => {
                let mut iter = hash_set.into_iter();

                let Some(first) = iter.next() else {
                    return Default::default();
                };

                let mut set = first.eval(provider);

                for input in iter {
                    set = set.intersection(&input.eval(provider)).cloned().collect();
                }

                set
            }
            SetExpr::Difference(input1, input2) => {
                let set1 = input1.eval(provider);
                let set2 = input2.eval(provider);
                set1.difference(&set2).cloned().collect()
            }

            SetExpr::All => {
                Self::Complement(Input::Expr(Box::new(Self::Union(Default::default())))) // complement of an empty union is the same as universe.
                    .eval(provider)
            }

            SetExpr::Complement(input) => provider
                .providers
                .cards
                .load_ids()
                .into_iter()
                .map(|id| MaybeCard::Id(id))
                .collect::<BTreeSet<MaybeCard>>()
                .difference(&input.eval(provider))
                .cloned()
                .collect(),
        }
    }
}
