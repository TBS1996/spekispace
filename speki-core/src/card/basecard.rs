use super::*;
use crate::{audio::AudioId, card_provider::CardProvider, CardProperty, CardRefType};
use either::Either;
use ledgerstore::{ItemReference, Ledger, LedgerItem, LedgerType, PropertyCache};
use omtrent::TimeStamp;
use serde::{Deserialize, Serialize, Serializer};
use std::{collections::HashSet, fmt::Display, str::FromStr};

pub type CardId = Uuid;

/// Text which might contain card references.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default, Ord, PartialOrd)]
pub struct TextData(Vec<Either<String, TextLink>>);

impl From<String> for TextData {
    fn from(value: String) -> Self {
        Self::from_raw(&value)
    }
}

impl TextData {
    pub fn extend(&mut self, other: Self) {
        for cmp in other.0 {
            self.0.push(cmp);
        }
    }

    pub fn push_eval(&mut self, eval: EvalText) {
        let mut x: Vec<Either<String, TextLink>> = Default::default();

        for cmp in eval.components() {
            match cmp {
                Either::Left(s) => x.push(Either::Left(s.to_owned())),
                Either::Right((s, id)) => {
                    let link = TextLink {
                        id: *id,
                        alias: Some(s.to_owned()),
                    };

                    x.push(Either::Right(link));
                }
            }
        }

        self.inner_mut().extend(x);
    }

    pub fn push_link(&mut self, id: CardId, alias: Option<String>) {
        let link = TextLink { id, alias };
        self.0.push(Either::Right(link));
    }

    pub fn push_string(&mut self, s: String) {
        self.0.push(Either::Left(s));
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn inner(&self) -> &Vec<Either<String, TextLink>> {
        &self.0
    }

    pub fn pop(&mut self) -> Option<Either<String, TextLink>> {
        self.0.pop()
    }

    pub fn inner_mut(&mut self) -> &mut Vec<Either<String, TextLink>> {
        &mut self.0
    }

    pub fn evaluate(&self, provider: &CardProvider) -> String {
        let mut out = String::new();

        for cmp in &self.0 {
            match cmp {
                Either::Left(s) => out.push_str(&s),
                Either::Right(TextLink { id, alias }) => match alias {
                    Some(alias) => out.push_str(&alias),
                    None => match provider.load(*id) {
                        Some(card) => out.push_str(&card.name),
                        None => out.push_str("<invalid card ref>"),
                    },
                },
            }
        }

        out
    }

    pub fn card_ids(&self) -> Vec<CardId> {
        let mut out = vec![];

        for cmp in &self.0 {
            match cmp {
                Either::Left(_) => {}
                Either::Right(TextLink { id, .. }) => out.push(*id),
            }
        }

        out
    }

    pub fn from_raw(input: &str) -> Self {
        let mut result = Vec::new();
        let mut buffer = String::new();
        let mut chars = input.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '[' && chars.peek() == Some(&'[') {
                chars.next(); // consume the second '['

                // Push any text before this link
                if !buffer.is_empty() {
                    result.push(Either::Left(std::mem::take(&mut buffer)));
                }

                // Parse until closing "]]"
                let mut link_buf = String::new();
                while let Some(ch) = chars.next() {
                    if ch == ']' && chars.peek() == Some(&']') {
                        chars.next(); // consume second ']'
                        break;
                    } else {
                        link_buf.push(ch);
                    }
                }

                let parts: Vec<&str> = link_buf.splitn(2, '|').collect();
                let (id_str, alias_opt) = if parts.len() == 2 {
                    (parts[0], Some(parts[1].to_string()))
                } else {
                    (parts[0], None)
                };

                match id_str.parse::<CardId>() {
                    Ok(id) => result.push(Either::Right(TextLink {
                        id,
                        alias: alias_opt,
                    })),
                    Err(_) => result.push(Either::Left(format!("[[{}]]", link_buf))),
                }
            } else {
                buffer.push(c);
            }
        }

        if !buffer.is_empty() {
            result.push(Either::Left(buffer));
        }

        Self(result)
    }

    pub fn to_raw(&self) -> String {
        let mut out = String::new();

        for cmp in &self.0 {
            let s = match cmp {
                Either::Left(s) => s.to_string(),
                Either::Right(TextLink { id, alias }) => match alias {
                    Some(alias) => format!("[[{id}|{alias}]]"),
                    None => format!("[[{id}]]"),
                },
            };

            out.push_str(&s);
        }

        out
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct TextLink {
    pub id: CardId,
    pub alias: Option<String>,
}

impl TextLink {
    pub fn new(id: CardId) -> Self {
        Self { id, alias: None }
    }
}

impl Serialize for TextData {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_raw())
    }
}

impl<'de> Deserialize<'de> for TextData {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(TextData::from_raw(&s))
    }
}

pub type AttributeId = Uuid;

/*

ok lets say

the backtype has to be an instance of programming function where codebase == rust.
hmm it feels kinda recursive in a way.. like can we make infinite


InstanceValue {
instance_of: {programming_function}
constraints: vec![
    InstanceFilter{
        attr: Codebase
        constraint:Card({rust})
}]
}


ok lets see, another one, all cards of type male whose father was born in germany after 1960

InstanceValue {
    instance_of: {male}
    constraints: vec![
        AttrFilter{
            attr: birth_date
            constraint: ValueOf::TsAfter(1960)
        },
        AttrFilter{
            attr: birthplace
            constraint: ValueOf::Card({germany})
        },
    ]
}


ok lets see, another one, all cards of type female whose mother was born in europe before 1945

InstanceValue {
    instance_of: {female}
    constraints: vec![
        AttrFilter{
            attr: birth_date
            constraint: ValueOf::TsBefore(1945)
        },
        AttrFilter{
            attr: birthplace
            constraint: ValueOf::InstanceOfClass( InstanceValue {
                instance_of: {country},
                constraints: vec![AttrFilter {
                    attr: continent,
                    constraint: ValueOf::Card(europe)
                }]
            })
        },
    ]
}


/// To ensure that a certain attribute has a certain value
struct AttrFilter {
    attr: Attrv2,
    constraint: ValueOf,
}

struct InstanceValue {
    instance_of: CardId,
    constraints: Vec<AttrFilter>,
}

/// like a filter to verify that the backside type/value is correct
enum ValueOf {
    Ts,                  // any timestamp
    TsBefore(TimeStamp), //timestamp before given time
    TsAfter(TimeStamp),  //timestamp after given time
    InstanceOfClass(InstanceValue),
    Card(CardId),
}

*/

#[derive(PartialEq, Debug, Clone, Serialize, Hash, Eq, Ord, PartialOrd)]
pub enum AttrBackType {
    InstanceOfClass(CardId),
    TimeStamp,
    Boolean,
}

impl AttrBackType {
    pub fn is_valid(
        &self,
        back_side: &BackSide,
        ledger: &LedgerType<RawCard>,
    ) -> Result<(), CardError> {
        match (self, back_side) {
            (AttrBackType::InstanceOfClass(instance), BackSide::Card(answer)) => {
                let mut parent_class: CardId = match ledger.load(*answer).unwrap().data {
                    CardType::Instance { class, .. } => class,
                    _ => return Err(CardError::WrongCardType),
                };

                while parent_class != *instance {
                    parent_class = match ledger.load(parent_class).unwrap().data {
                        CardType::Class {
                            parent_class: Some(class),
                            ..
                        } => class,
                        CardType::Class {
                            parent_class: None, ..
                        } => return Err(CardError::InstanceOfNonClass),
                        _ => return Err(CardError::WrongCardType),
                    };
                }

                Ok(())
            }
            (AttrBackType::InstanceOfClass(_), _) => Err(CardError::AnswerMustBeCard),
            (AttrBackType::TimeStamp, BackSide::Time(_)) => Ok(()),
            (AttrBackType::TimeStamp, _) => Err(CardError::AnswerMustBeTime),
            (AttrBackType::Boolean, BackSide::Bool(_)) => Ok(()),
            (AttrBackType::Boolean, _) => Err(CardError::AnswerMustBeBool),
        }
    }
}

impl<'de> Deserialize<'de> for AttrBackType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;

        // Try to deserialize directly as Uuid
        if let Ok(uuid) = Uuid::deserialize(&value) {
            return Ok(AttrBackType::InstanceOfClass(uuid));
        }

        // Try deserializing as the new enum using a helper
        #[derive(Deserialize)]
        enum Helper {
            InstanceOfClass(CardId),
            TimeStamp,
            Boolean,
        }

        let helper: Helper = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
        match helper {
            Helper::InstanceOfClass(id) => Ok(AttrBackType::InstanceOfClass(id)),
            Helper::TimeStamp => Ok(AttrBackType::TimeStamp),
            Helper::Boolean => Ok(AttrBackType::Boolean),
        }
    }
}

/// An attribute of a class is pre-made questions that can be asked about any of the classes' instances.
/// For example, all instances of `Person` can have the quesiton "when was {} born?"
///
/// Instances refer to both direct instances of the class, and instances of any sub-classes of the class.
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialOrd, Ord)]
pub struct Attrv2 {
    pub id: AttributeId,
    pub pattern: String,
    pub back_type: Option<AttrBackType>,
}

pub enum BackSideConstraint {
    Card { ty: Option<CardId> },
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialOrd, Ord)]
pub struct ParamAnswer {
    pub answer: BackSide,
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, Hash, Eq)]
pub enum CardType {
    /// A specific instance of a class
    /// For example, the instance might be Elvis Presley where the concept would be "Person"
    Instance {
        name: TextData,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        back: Option<BackSide>,
        class: CardId,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        answered_params: BTreeMap<AttributeId, ParamAnswer>,
    },
    Normal {
        front: TextData,
        back: BackSide,
    },
    Unfinished {
        front: TextData,
    },
    /// An attribute describes a specific instance of a class. For example the class Person can have attribute "when was {} born?"
    /// this will be applied to all instances of the class and its subclasses
    Attribute {
        attribute: AttributeId,
        back: BackSide,
        instance: CardId,
    },

    /// A class, which is something that has specific instances of it, but is not a single thing in itself.
    /// A class might also have sub-classes, for example, the class chemical element has a sub-class isotope
    Class {
        name: TextData,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        back: Option<BackSide>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent_class: Option<CardId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default_question: Option<TextData>,
        #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
        attrs: BTreeSet<Attrv2>,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        params: BTreeMap<AttributeId, Attrv2>,
    },

    /// A statement is a fact which cant easily be represented with a flashcard,
    /// because asking the question implies the answer.
    ///
    /// For example, "Can the anglerfish produce light?" is a dumb question because it's so rare for animals
    /// to produce light that the question wouldn't have been asked if it wasn't true.
    ///
    /// For these questions we use a statementcard which will simply state the fact without asking you. We still
    /// need this card for dependency management since other questions might rely on you knowing this fact.
    /// Knowledge of these kinda facts will instead be measured indirectly with questions about this property
    ///
    /// More formal definition of when a statement card is used:
    ///
    /// 1. It represents a property of an instance or sub-class.
    /// 2. The set of the class it belongs to is large
    /// 3. The property in that set is rare, but not unique
    Statement {
        front: TextData,
    },

    /// gotta figure out if i want this to be a thing in itself or it can be handled with just attributes of an event class
    Event {
        front: TextData,
        start_time: TimeStamp,
        end_time: Option<TimeStamp>,
        parent_event: Option<CardId>,
    },
}

impl CardType {
    pub fn class(&self) -> Option<CardId> {
        match self {
            CardType::Instance { class, .. } => Some(*class),
            CardType::Normal { .. } => None,
            CardType::Unfinished { .. } => None,
            CardType::Attribute { .. } => None,
            CardType::Class { parent_class, .. } => *parent_class,
            CardType::Statement { .. } => None,
            CardType::Event { .. } => None,
        }
    }

    pub fn backside(&self) -> Option<&BackSide> {
        match self {
            CardType::Instance {
                back: Some(back), ..
            } => {
                if back.is_empty_text() {
                    None
                } else {
                    Some(back)
                }
            }
            CardType::Instance { back: None, .. } => None,
            CardType::Normal { back, .. } => Some(back),
            CardType::Unfinished { .. } => None,
            CardType::Attribute { back, .. } if !back.is_empty_text() => Some(back),
            CardType::Attribute { back, .. } => Some(back),
            CardType::Class {
                back: Some(back), ..
            } if !back.is_empty_text() => Some(back),
            CardType::Class {
                back: Some(back), ..
            } => Some(back),
            CardType::Class { back: None, .. } => None,
            CardType::Statement { .. } => None,
            CardType::Event { .. } => None,
        }
    }

    pub fn raw_front(&self) -> String {
        match self.clone() {
            CardType::Instance { name, .. } => name.to_raw(),
            CardType::Normal { front, .. } => front.to_raw(),
            CardType::Unfinished { front } => front.to_raw(),
            CardType::Attribute { .. } => "attr card".to_string(),
            CardType::Class { name, .. } => name.to_raw(),
            CardType::Statement { front } => front.to_raw(),
            CardType::Event { front, .. } => front.to_raw(),
        }
    }

    pub fn raw_back(&self) -> String {
        self.backside().map(|x| x.to_string()).unwrap_or_default()
    }

    pub async fn get_dependencies(&self) -> BTreeSet<CardId> {
        match self {
            CardType::Instance { class, back, .. } => {
                let mut dependencies: BTreeSet<CardId> = Default::default();
                dependencies.insert(*class);
                dependencies.extend(
                    back.clone()
                        .map(|x| x.dependencies())
                        .unwrap_or_default()
                        .iter(),
                );
                dependencies
            }
            CardType::Normal { .. } => Default::default(),
            CardType::Unfinished { .. } => Default::default(),
            CardType::Attribute { back, instance, .. } => {
                let mut dependencies: BTreeSet<CardId> = Default::default();
                dependencies.insert(*instance);
                dependencies.extend(back.dependencies().iter());
                dependencies
            }
            CardType::Class {
                back, parent_class, ..
            } => {
                let mut dependencies: BTreeSet<CardId> = Default::default();
                dependencies.extend(back.as_ref().map(|x| x.dependencies()).unwrap_or_default());
                if let Some(id) = parent_class {
                    dependencies.insert(*id);
                }
                dependencies
            }
            CardType::Statement { .. } => Default::default(),
            CardType::Event { .. } => todo!(),
        }
    }

    pub fn name_fixed_ledger(&self) -> TextData {
        match self {
            CardType::Instance { name, .. } => name.clone(),
            CardType::Normal { front, .. } => front.clone(),
            CardType::Unfinished { front, .. } => front.clone(),
            CardType::Attribute { .. } => {
                panic!()
            }
            CardType::Class { name, .. } => name.clone(),
            CardType::Statement { front, .. } => front.clone(),
            CardType::Event { front, .. } => front.clone(),
        }
    }

    pub fn name(&self, provider: &CardProvider) -> TextData {
        match self {
            CardType::Instance { name, .. } => name.clone(),
            CardType::Normal { front, .. } => front.clone(),
            CardType::Unfinished { front, .. } => front.clone(),
            CardType::Attribute {
                attribute,
                instance,
                ..
            } => {
                let class: CardId = provider
                    .providers
                    .cards
                    .get_prop_cache(PropertyCache::new(
                        CardProperty::Attr,
                        attribute.to_string(),
                    ))
                    .into_iter()
                    .next()
                    .unwrap();

                let class = provider.load(class).unwrap();
                let attr = class.get_attr(*attribute).unwrap();
                let instance = provider.load(*instance).unwrap().name_textdata();
                let instance = instance.to_raw();

                let new = attr.pattern.replace("{}", &instance);

                TextData::from_raw(&new)
            }
            CardType::Class { name, .. } => name.clone(),
            CardType::Statement { front, .. } => front.clone(),
            CardType::Event { front, .. } => front.clone(),
        }
    }

    pub fn param_to_ans(&self, provider: &CardProvider) -> BTreeMap<Attrv2, Option<ParamAnswer>> {
        if let CardType::Instance {
            answered_params,
            class,
            ..
        } = self
        {
            let class = provider.load(*class).unwrap();

            let mut params = class.params_on_class();
            params.extend(class.params_on_parent_classes().values().flatten().cloned());

            let mut out: BTreeMap<Attrv2, Option<ParamAnswer>> = Default::default();

            for param in params {
                match answered_params.get(&param.id) {
                    Some(ans) => out.insert(param, Some(ans.to_owned())),
                    None => out.insert(param, None),
                };
            }

            out
        } else {
            Default::default()
        }
    }

    fn class_stuff(&self, class: CardId, provider: &CardProvider) -> String {
        let mut class_name = match provider.providers.cards.load(class).unwrap().data.clone() {
            CardType::Class { name, .. } => name,
            other => {
                dbg!(class);
                dbg!(other);
                panic!();
            }
        };

        let mut segments: Vec<String> = Default::default();

        let mut params: Vec<(Attrv2, Option<ParamAnswer>)> =
            self.param_to_ans(provider).into_iter().collect();

        params.sort_by_key(|p| p.0.id);

        for (_, answer) in self.param_to_ans(provider) {
            let val = match answer {
                Some(p) => EvalText::from_backside(&p.answer, provider, false, true).to_string(),
                None => EvalText::just_some_string("_".to_owned(), provider).to_string(),
            };

            let segment = format!("{}", val);
            segments.push(segment);
        }

        if !segments.is_empty() {
            let segments: String = segments.join(", ");
            class_name.push_string(format!("<{}>", segments));
        }

        class_name.evaluate(provider)
    }

    fn class_stuff_from_instance(&self, instance: CardId, provider: &CardProvider) -> String {
        let class = provider.load(instance).unwrap().class().unwrap();
        self.class_stuff(class, provider)
    }

    pub fn display_front(&self, provider: &CardProvider) -> TextData {
        match self {
            CardType::Instance { name, class, .. } => {
                let thename = &name.evaluate(provider);
                let class_name = self.class_stuff(*class, provider);
                let s = format!("{thename} ({class_name})");
                TextData::from_raw(&s)
            }
            CardType::Normal { front, .. } => front.clone(),
            CardType::Unfinished { front, .. } => front.clone(),
            CardType::Attribute {
                attribute,
                instance,
                ..
            } => {
                let class: CardId = provider
                    .providers
                    .cards
                    .get_prop_cache(PropertyCache::new(
                        CardProperty::Attr,
                        attribute.to_string(),
                    ))
                    .into_iter()
                    .next()
                    .unwrap();

                let class = provider.load(class).unwrap();

                let attr = class.get_attr(*attribute).unwrap();

                let class_name = self.class_stuff_from_instance(*instance, provider);

                let new = if attr.pattern.contains("{}") {
                    attr.pattern.replace("{}", &format!("[[{instance}]]"))
                } else {
                    format!("[[{instance}]]({}): {}", class_name, attr.pattern)
                };

                TextData::from_raw(&new)
            }
            CardType::Class {
                name, parent_class, ..
            } => match parent_class {
                Some(class) => {
                    let mut name = name.clone();

                    if self.backside().is_some() {
                        name.push_string(" ( ".to_string());
                        name.push_link(*class, None);
                        name.push_string(")".to_string());
                    }

                    name
                }
                None => name.clone(),
            },
            CardType::Statement { front, .. } => front.clone(),
            CardType::Event { front, .. } => front.clone(),
        }
    }

    pub fn type_name(&self) -> &str {
        match self {
            CardType::Unfinished { .. } => "unfinished",
            CardType::Statement { .. } => "statement",
            CardType::Attribute { .. } => "attribute",
            CardType::Instance { .. } => "instance",
            CardType::Normal { .. } => "normal",
            CardType::Class { .. } => "class",
            CardType::Event { .. } => "event",
        }
    }

    /// This is mainly just so i dont forget to update the CType when the AnyType changes
    pub fn fieldless(&self) -> CType {
        match self {
            CardType::Instance { .. } => CType::Instance,
            CardType::Normal { .. } => CType::Normal,
            CardType::Unfinished { .. } => CType::Unfinished,
            CardType::Attribute { .. } => CType::Attribute,
            CardType::Class { .. } => CType::Class,
            CardType::Statement { .. } => CType::Statement,
            CardType::Event { .. } => CType::Event,
        }
    }

    pub fn parent_class(&self) -> Option<CardId> {
        match self {
            CardType::Class { parent_class, .. } => *parent_class,
            _ => None,
        }
    }

    pub fn is_class(&self) -> bool {
        matches!(self, Self::Class { .. })
    }
    pub fn is_instance(&self) -> bool {
        matches!(self, Self::Instance { .. })
    }
    pub fn is_finished(&self) -> bool {
        !matches!(self, Self::Unfinished { .. })
    }
}

fn bool_is_false(b: &bool) -> bool {
    *b == false
}

#[derive(Serialize, Deserialize, Debug, Clone, Hash, PartialEq, Eq)]
pub struct RawCard {
    pub id: Uuid,
    /// The context of which the name of the card makes sense. For example, instead of writing `kubernetes node`, you can just
    /// write `node` and put kubernetes as the namespace. This avoids unnecessarily long names for disambiguation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<CardId>,
    pub data: CardType,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub explicit_dependencies: BTreeSet<Uuid>,
    #[serde(default, skip_serializing_if = "bool_is_false")]
    pub trivial: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tags: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub front_audio: Option<AudioId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub back_audio: Option<AudioId>,
}

impl RawCard {
    pub fn into_events(self) -> Vec<CardEvent> {
        let mut actions: Vec<CardAction> = vec![];

        let Self {
            id,
            namespace,
            data,
            explicit_dependencies,
            trivial: _,
            tags: _,
            front_audio: _,
            back_audio: _,
        } = self;

        match data {
            CardType::Instance {
                name,
                back,
                class,
                answered_params,
            } => {
                let action = CardAction::InstanceType {
                    front: name,
                    class: class,
                };

                actions.push(action);
                actions.push(CardAction::SetBackside(back));
                actions.push(CardAction::InsertParamAnswers(answered_params));
            }
            CardType::Normal { front, back } => {
                actions.push(CardAction::NormalType { front, back });
            }
            CardType::Unfinished { front } => actions.push(CardAction::UnfinishedType { front }),
            CardType::Attribute {
                attribute,
                back,
                instance,
            } => {
                actions.push(CardAction::AttributeType {
                    attribute,
                    back,
                    instance,
                });
            }
            CardType::Class {
                name,
                back,
                parent_class,
                default_question: _,
                attrs,
                params,
            } => {
                actions.push(CardAction::ClassType { front: name });
                actions.push(CardAction::SetParentClass(parent_class));
                actions.push(CardAction::SetBackside(back));
                actions.push(CardAction::InsertAttrs(attrs));
                actions.push(CardAction::InsertParams(params.into_values().collect()));
            }
            CardType::Statement { front } => {
                actions.push(CardAction::StatementType { front });
            }
            CardType::Event {
                front,
                start_time,
                end_time: _,
                parent_event: _,
            } => {
                actions.push(CardAction::EventType { front, start_time });
            }
        }

        actions.push(CardAction::SetNamespace(namespace));

        for dep in explicit_dependencies {
            actions.push(CardAction::AddDependency(dep));
        }

        let mut events: Vec<CardEvent> = vec![];

        for action in actions {
            let event = CardEvent::new_modify(id, action);
            events.push(event);
        }

        events
    }

    pub fn cache_front(&self, ledger: &Ledger<RawCard>) -> String {
        match self.data.clone() {
            CardType::Instance { name, .. } => name.to_raw(),
            CardType::Normal { front, .. } => front.to_raw(),
            CardType::Unfinished { front } => front.to_raw(),
            CardType::Attribute {
                attribute: _,
                instance,
                ..
            } => {
                let attr = self.get_attr_rec(ledger.to_owned()).unwrap();

                let instance = ledger.load(instance).unwrap().data.name_fixed_ledger();
                let instance = instance.to_raw();

                let new = attr.pattern.replace("{}", &instance);
                new
            }
            CardType::Class { name, .. } => name.to_raw(),
            CardType::Statement { front } => front.to_raw(),
            CardType::Event { front, .. } => front.to_raw(),
        }
    }

    /// Returns the class this card belongs to (if any)
    pub fn parent_class(&self) -> Option<CardId> {
        match self.data {
            CardType::Instance { class, .. } => Some(class),
            CardType::Normal { .. } => None,
            CardType::Unfinished { .. } => None,
            CardType::Attribute { .. } => None,
            CardType::Class { parent_class, .. } => parent_class,
            CardType::Statement { .. } => None,
            CardType::Event { .. } => None,
        }
    }

    pub fn ref_backside(&self) -> Option<&BackSide> {
        match &self.data {
            CardType::Instance { back, .. } => back.as_ref(),
            CardType::Normal { back, .. } => Some(back),
            CardType::Unfinished { .. } => None,
            CardType::Attribute { back, .. } => Some(back),
            CardType::Class { back, .. } => back.as_ref(),
            CardType::Statement { .. } => None,
            CardType::Event { .. } => None,
        }
    }

    fn params(&self) -> BTreeMap<AttributeId, Attrv2> {
        if let CardType::Class { ref params, .. } = &self.data {
            return params.clone();
        } else {
            Default::default()
        }
    }

    fn attrs(&self) -> BTreeSet<Attrv2> {
        if let CardType::Class { ref attrs, .. } = &self.data {
            return attrs.clone();
        } else {
            Default::default()
        }
    }

    pub fn get_attr(&self, id: AttributeId) -> Option<Attrv2> {
        if let CardType::Class { ref attrs, .. } = &self.data {
            attrs.iter().find(|attr| attr.id == id).cloned()
        } else {
            None
        }
    }

    pub fn get_attr_rec(&self, ledger: Ledger<RawCard>) -> Option<Attrv2> {
        let CardType::Attribute {
            attribute,
            instance,
            ..
        } = &self.data
        else {
            return None;
        };

        let mut card: Arc<Self> = ledger.load(*instance).unwrap();

        while let Some(parent) = card.parent_class() {
            card = ledger.load(parent).unwrap();
            if let Some(attr) = card.get_attr(*attribute) {
                return Some(attr);
            }
        }

        None
    }

    pub fn set_backside(mut self, new_back: BackSide) -> Self {
        let data = match self.data.clone() {
            x @ CardType::Event { .. } => x,
            CardType::Instance {
                name,
                back: _,
                class,
                answered_params,
            } => CardType::Instance {
                name,
                back: Some(new_back),
                class,
                answered_params,
            },
            x @ CardType::Statement { .. } => x,

            CardType::Normal { front, back: _ } => CardType::Normal {
                front,
                back: new_back,
            },
            CardType::Unfinished { front } => CardType::Normal {
                front,
                back: new_back,
            },
            CardType::Attribute {
                attribute,
                instance: concept_card,
                back: _,
            } => CardType::Attribute {
                attribute,
                back: new_back,
                instance: concept_card,
            },
            CardType::Class {
                name,
                back: _,
                parent_class,
                default_question,
                attrs,
                params,
            } => CardType::Class {
                name,
                back: Some(new_back),
                parent_class,
                default_question,
                attrs,
                params,
            },
        };

        self.data = data;
        self
    }
}

pub fn bigrams(text: &str) -> Vec<[char; 2]> {
    normalize_string(text)
        .chars()
        .collect::<Vec<_>>()
        .windows(2)
        .map(|w| [w[0], w[1]])
        .collect()
}

pub fn normalize_string(str: &str) -> String {
    deunicode::deunicode(str)
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

use fancy_regex::Regex;
fn resolve_text(txt: String, ledger: &Ledger<RawCard>, re: &Regex) -> String {
    let uuids: Vec<CardId> = re
        .find_iter(&txt)
        .filter_map(Result::ok)
        .map(|m| m.as_str().parse().unwrap())
        .collect();

    let mut s: String = re.replace_all(&txt, "").to_string();
    for id in uuids {
        let card = ledger.load(id).unwrap();
        let txt = card.cache_front(ledger);
        s.push_str(&resolve_text(txt, ledger, re));
    }

    s
}

/// replaces all uuids on frontside of card with the frontside of the card referenced by uuid.
/// just appends it, doesn't preserve order, this is just to collect bigrams.
fn resolve_card(card: &RawCard, ledger: &Ledger<RawCard>) -> String {
    let uuid_regex = Regex::new(
        r"\b[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\b",
    )
    .unwrap();

    resolve_text(card.cache_front(ledger), ledger, &uuid_regex)
}

#[derive(Debug)]
pub enum CardError {
    MissingParam,
    InstanceOfNonClass,
    AttributeOfNonInstance,
    MissingAttribute,
    DefaultQuestionNotClass,
    WrongCardType,
    AnswerMustBeCard,
    AnswerMustBeTime,
    AnswerMustBeBool,
    SubClassOfNonClass,
    BackTypeMustBeClass,
}

fn instance_is_of_type(instance: CardId, ty: CardId, ledger: &LedgerType<RawCard>) -> bool {
    let instance = ledger.load(instance).unwrap();
    assert!(instance.data.is_instance());

    get_parent_classes(ty, ledger)
        .into_iter()
        .find(|class| class.id == ty)
        .is_some()
}

fn get_parent_classes(class: CardId, ledger: &LedgerType<RawCard>) -> Vec<Arc<RawCard>> {
    let class = ledger.load(class).unwrap();
    let mut classes: Vec<Arc<RawCard>> = vec![class.clone()];
    assert!(class.data.is_class());
    let mut parent_class = class.parent_class();

    while let Some(parent) = parent_class {
        let class = ledger.load(parent).unwrap();
        assert!(class.data.is_class());
        parent_class = class.parent_class();
        classes.push(class);
    }

    classes
}

fn get_attributes(class: CardId, ledger: &LedgerType<RawCard>) -> Vec<Attrv2> {
    let mut out: Vec<Attrv2> = vec![];
    for class in get_parent_classes(class, ledger) {
        if let CardType::Class { attrs, .. } = &class.data {
            out.extend(attrs.clone());
        } else {
            panic!()
        }
    }
    out
}

impl LedgerItem for RawCard {
    type Error = CardError;
    type Key = CardId;
    type RefType = CardRefType;
    type PropertyType = CardProperty;
    type Modifier = CardAction;

    fn validate(&self, ledger: &LedgerType<Self>) -> Result<(), Self::Error> {
        match &self.data {
            CardType::Instance {
                name: _,
                back: _,
                class,
                answered_params,
            } => {
                let mut class: Option<Arc<RawCard>> = Some(ledger.load(*class).unwrap());
                let mut recursive_params: BTreeMap<AttributeId, Attrv2> = Default::default();

                while let Some(the_class) = class.clone() {
                    let CardType::Class {
                        params,
                        parent_class,
                        ..
                    } = &the_class.data
                    else {
                        return Err(CardError::InstanceOfNonClass);
                    };

                    recursive_params.extend(params.clone());
                    match parent_class {
                        Some(parent_class) => class = Some(ledger.load(*parent_class).unwrap()),
                        None => class = None,
                    }
                }

                for (key_ans, val_ans) in answered_params {
                    match recursive_params.get(key_ans).as_ref() {
                        Some(p) => {
                            if let Some(back_type) = &p.back_type {
                                back_type.is_valid(&val_ans.answer, ledger)?;
                            }
                        }
                        None => return Err(CardError::MissingParam),
                    }
                }
            }

            CardType::Normal { front: _, back: _ } => {}
            CardType::Unfinished { front: _ } => {}
            CardType::Attribute {
                attribute,
                back: attr_back,
                instance,
            } => {
                let CardType::Instance {
                    name,
                    back,
                    class,
                    answered_params: _,
                } = ledger.load(*instance).unwrap().data.clone()
                else {
                    return Err(CardError::InstanceOfNonClass);
                };

                let class = {
                    let class = ledger.load(class).unwrap();
                    if !class.data.is_class() {
                        return Err(CardError::InstanceOfNonClass);
                    }
                    class
                };

                let Some(Attrv2 { back_type, .. }) = get_attributes(class.id, &ledger)
                    .into_iter()
                    .find(|attr| attr.id == *attribute)
                else {
                    return Err(CardError::MissingAttribute);
                };

                match back_type {
                    Some(AttrBackType::Boolean) => {
                        if !matches!(attr_back, BackSide::Bool(_)) {
                            return Err(CardError::AnswerMustBeBool);
                        }
                    }
                    Some(AttrBackType::TimeStamp) => {
                        if !matches!(attr_back, BackSide::Time(_)) {
                            return Err(CardError::AnswerMustBeTime);
                        }
                    }
                    Some(AttrBackType::InstanceOfClass(back_class)) => {
                        if let BackSide::Card(answer) = attr_back {
                            if !instance_is_of_type(*answer, back_class, &ledger) {
                                return Err(CardError::WrongCardType);
                            }
                        } else {
                            dbg!(name);
                            dbg!(&back);
                            dbg!(&attr_back);
                            dbg!(ledger.load(*instance));
                            dbg!(self);
                            let err = dbg!(CardError::AnswerMustBeCard);
                            return Err(err);
                        }
                    }
                    None => {}
                }
            }
            CardType::Class {
                name: _,
                back: _,
                parent_class,
                default_question: _,
                attrs,
                params: _,
            } => {
                if let Some(parent) = parent_class {
                    if !ledger.load(*parent).unwrap().data.is_class() {
                        return Err(CardError::SubClassOfNonClass);
                    }
                }

                for attr in attrs {
                    if let Some(AttrBackType::InstanceOfClass(back_type)) = attr.back_type {
                        if !ledger.load(back_type).unwrap().data.is_class() {
                            return Err(CardError::BackTypeMustBeClass);
                        }
                    }
                }
            }
            CardType::Statement { front: _ } => {}
            // todo lol
            CardType::Event {
                front: _,
                start_time: _,
                end_time: _,
                parent_event: _,
            } => {}
        }

        Ok(())
    }

    fn ref_cache(&self) -> HashSet<ItemReference<Self>> {
        let from = self.id;
        let mut out: HashSet<ItemReference<Self>> = Default::default();

        fn refs_from_backside(from: CardId, back: &BackSide) -> HashSet<ItemReference<RawCard>> {
            let mut out: HashSet<ItemReference<RawCard>> = Default::default();
            match back {
                BackSide::Text(txt) => {
                    for id in txt.card_ids() {
                        out.insert(ItemReference::new(from, id, CardRefType::LinkRef));
                    }
                }
                BackSide::Card(id) => {
                    out.insert(ItemReference::new(from, *id, CardRefType::LinkRef));
                }
                BackSide::List(ids) => {
                    for id in ids {
                        out.insert(ItemReference::new(from, *id, CardRefType::LinkRef));
                    }
                }
                BackSide::Time(_) => {}
                BackSide::Trivial => {}
                BackSide::Invalid => {}
                BackSide::Bool(_) => {}
            }
            out
        }

        if let Some(ns) = self.namespace {
            out.insert(ItemReference::new(from, ns, CardRefType::LinkRef));
        }

        for dep in &self.explicit_dependencies {
            out.insert(ItemReference::new(
                from,
                *dep,
                CardRefType::ExplicitDependency,
            ));
        }

        match &self.data {
            CardType::Normal { front, .. } => {
                for id in front.card_ids() {
                    out.insert(ItemReference::new(from, id, CardRefType::LinkRef));
                }
            }
            CardType::Unfinished { front, .. } => {
                for id in front.card_ids() {
                    out.insert(ItemReference::new(from, id, CardRefType::LinkRef));
                }
            }
            CardType::Instance {
                name,
                class,
                answered_params,
                ..
            } => {
                for id in name.card_ids() {
                    out.insert(ItemReference::new(from, id, CardRefType::LinkRef));
                }

                out.insert(ItemReference::new(
                    from,
                    *class,
                    CardRefType::ClassOfInstance,
                ));

                for (_, ans) in answered_params.iter() {
                    out.extend(refs_from_backside(from, &ans.answer));
                }
            }
            CardType::Attribute { instance, .. } => {
                out.insert(ItemReference::new(
                    from,
                    *instance,
                    CardRefType::InstanceOfAttribute,
                ));
            }
            CardType::Class {
                name,
                default_question,
                parent_class,
                ..
            } => {
                for id in name.card_ids() {
                    out.insert(ItemReference::new(from, id, CardRefType::LinkRef));
                }

                if let Some(def) = default_question {
                    for id in def.card_ids() {
                        out.insert(ItemReference::new(from, id, CardRefType::LinkRef));
                    }
                }

                if let Some(class) = parent_class {
                    out.insert(ItemReference::new(from, *class, CardRefType::ParentClass));
                }
            }
            CardType::Statement { front, .. } => {
                for id in front.card_ids() {
                    out.insert(ItemReference::new(from, id, CardRefType::LinkRef));
                }
            }
            CardType::Event { front, .. } => {
                for id in front.card_ids() {
                    out.insert(ItemReference::new(from, id, CardRefType::LinkRef));
                }
            }
        };

        if let Some(back) = &self.data.backside() {
            out.extend(refs_from_backside(from, back));
        }

        out
    }

    fn properties_cache(&self, cache: &Ledger<Self>) -> HashSet<PropertyCache<Self>> {
        let mut out: HashSet<PropertyCache<Self>> = Default::default();

        let resolved_text = resolve_card(self, &cache);

        for bigram in bigrams(&resolved_text) {
            let value = format!("{}{}", bigram[0], bigram[1]);
            let prop = PropertyCache {
                property: CardProperty::Bigram,
                value,
            };
            out.insert(prop);
        }

        if self.trivial {
            out.insert(PropertyCache {
                property: CardProperty::Trivial,
                value: self.trivial.to_string(),
            });
        }

        if self.data.backside().is_some() && !matches!(&self.data, CardType::Unfinished { .. }) {
            out.insert(PropertyCache {
                property: CardProperty::Reviewable,
                value: true.to_string(),
            });
        }

        match &self.data {
            CardType::Normal { .. } => {}
            CardType::Unfinished { .. } => {}
            CardType::Instance { .. } => {}
            CardType::Attribute { attribute, .. } => {
                let prop = PropertyCache {
                    property: CardProperty::AttrId,
                    value: attribute.to_string(),
                };
                out.insert(prop);
            }
            CardType::Class { attrs, .. } => {
                for attr in attrs {
                    let prop = PropertyCache {
                        property: CardProperty::Attr,
                        value: attr.id.to_string(),
                    };
                    out.insert(prop);
                }
            }
            CardType::Statement { .. } => {}
            CardType::Event { .. } => {}
        };

        let val = format!("{:?}", self.data.fieldless());
        let prop = PropertyCache {
            property: CardProperty::CardType,
            value: val,
        };

        out.insert(prop);

        out
    }

    fn new_default(id: CardId) -> Self {
        Self {
            id,
            namespace: None,
            data: CardType::Unfinished {
                front: TextData::from_raw("uninit"),
            },
            trivial: false,
            tags: Default::default(),
            explicit_dependencies: Default::default(),
            front_audio: Default::default(),
            back_audio: Default::default(),
        }
    }

    fn inner_run_event(mut self, event: CardAction) -> Result<Self, Self::Error> {
        match event {
            CardAction::SetDefaultQuestion(default) => match &mut self.data {
                CardType::Class {
                    ref mut default_question,
                    ..
                } => *default_question = default.map(|s| TextData::from_raw(&s)),
                _ => return Err(CardError::DefaultQuestionNotClass),
            },
            CardAction::SetBackTime(ts) => {
                self = self.set_backside(BackSide::Time(ts));
            }
            CardAction::InsertParams(params) => {
                let new_params: BTreeMap<AttributeId, Attrv2> =
                    params.into_iter().map(|attr| (attr.id, attr)).collect();

                let CardType::Class { ref mut params, .. } = &mut self.data else {
                    return Err(CardError::WrongCardType);
                };

                *params = new_params;
            }
            CardAction::InsertParamAnswers(new_answered_params) => {
                let CardType::Instance {
                    ref mut answered_params,
                    ..
                } = &mut self.data
                else {
                    return Err(CardError::WrongCardType);
                };

                *answered_params = new_answered_params;
            }
            CardAction::SetTrivial(flag) => {
                self.trivial = flag;
            }
            CardAction::SetBackBool(b) => {
                let backside = BackSide::Bool(b);
                self = self.set_backside(backside);
            }
            CardAction::SetFrontAudio(audio) => {
                self.front_audio = audio;
            }
            CardAction::SetBackAudio(audio) => {
                self.back_audio = audio;
            }
            CardAction::UpsertCard(ty) => {
                self.data = ty;
            }
            CardAction::SetNamespace(ns) => {
                self.namespace = ns;
            }
            CardAction::AddDependency(dependency) => {
                self.explicit_dependencies.insert(dependency);
            }
            CardAction::RemoveDependency(dependency) => {
                self.explicit_dependencies.remove(&dependency);
            }
            CardAction::SetBackRef(reff) => {
                let backside = BackSide::Card(reff);
                self = self.set_backside(backside);
            }
            CardAction::InsertAttr(attrv2) => {
                if let CardType::Class { ref mut attrs, .. } = self.data {
                    attrs.insert(attrv2);
                } else {
                    panic!("expeted class");
                }
            }
            CardAction::RemoveParam(param) => {
                if let CardType::Class { ref mut params, .. } = self.data {
                    dbg!(&params);
                    debug_assert!(params.remove(&param).is_some());
                } else {
                    panic!("expeted class");
                }
            }
            CardAction::RemoveAttr(attr_id) => {
                if let CardType::Class { ref mut attrs, .. } = self.data {
                    dbg!(&attrs);
                    let attr_len = attrs.len();
                    attrs.retain(|attr| attr.id != attr_id);
                    assert_eq!(attr_len - 1, attrs.len());
                    dbg!(&attrs);
                } else {
                    panic!("expeted class");
                }
            }
            CardAction::SetParentClass(new_parent_class) => {
                if let CardType::Class {
                    ref mut parent_class,
                    ..
                } = self.data
                {
                    *parent_class = new_parent_class;
                } else {
                    panic!("expeted class");
                }
            }
            CardAction::SetInstanceClass(instance_class) => {
                if let CardType::Instance { ref mut class, .. } = self.data {
                    *class = instance_class;
                } else {
                    panic!("expected instance");
                }
            }
            CardAction::AttributeType {
                attribute,
                back,
                instance,
            } => {
                self.data = CardType::Attribute {
                    attribute,
                    back,
                    instance,
                };
            }
            CardAction::NormalType { front, back } => {
                self.data = CardType::Normal { front, back };
            }
            CardAction::InstanceType { front, class } => {
                let back = self.ref_backside().cloned();
                self.data = CardType::Instance {
                    name: front,
                    back,
                    class,
                    answered_params: Default::default(),
                };
            }
            CardAction::StatementType { front } => {
                self.data = CardType::Statement { front };
            }
            CardAction::ClassType { front } => {
                self.data = CardType::Class {
                    name: front,
                    back: self.ref_backside().cloned(),
                    parent_class: self.parent_class(),
                    default_question: None,
                    attrs: self.attrs(),
                    params: self.params(),
                };
            }
            CardAction::UnfinishedType { front } => {
                self.data = CardType::Unfinished { front };
            }
            CardAction::EventType { front, start_time } => {
                self.data = CardType::Event {
                    front,
                    start_time,
                    end_time: None,
                    parent_event: None,
                };
            }
            CardAction::SetBackside(back_side) => match &mut self.data {
                CardType::Instance { ref mut back, .. } => {
                    *back = back_side;
                }
                CardType::Normal { ref mut back, .. } => {
                    if let Some(back_side) = back_side {
                        *back = back_side;
                    } else {
                        panic!("normal cards require backside");
                    }
                }
                CardType::Unfinished { .. } => {
                    panic!("nope, unfinishde");
                }
                CardType::Attribute { ref mut back, .. } => {
                    if let Some(back_side) = back_side {
                        *back = back_side;
                    } else {
                        panic!("attr cards require backside");
                    }
                }
                CardType::Class { ref mut back, .. } => {
                    *back = back_side;
                }
                CardType::Statement { .. } => panic!("no back on statement"),
                CardType::Event { .. } => panic!("no back on event"),
            },
            CardAction::InsertAttrs(new_attrs) => {
                if let CardType::Class { ref mut attrs, .. } = self.data {
                    *attrs = new_attrs;
                } else {
                    panic!("expected class");
                }
            }
            CardAction::SetFront(new_front) => match &mut self.data {
                CardType::Instance { ref mut name, .. } => {
                    *name = new_front;
                }
                CardType::Normal { front, .. } => {
                    *front = new_front;
                }
                CardType::Unfinished { front } => {
                    *front = new_front;
                }
                CardType::Attribute { .. } => {
                    panic!("cant set frontside on attr cards")
                }
                CardType::Class { name, .. } => {
                    *name = new_front;
                }
                CardType::Statement { front } => *front = new_front,
                CardType::Event { front, .. } => {
                    *front = new_front;
                }
            },
        };

        let implicit_deps: BTreeSet<Uuid> = {
            let mut all = self.ref_cache();
            all.retain(|ItemReference { ty, .. }| match ty {
                CardRefType::ExplicitDependency => false,
                _ => true,
            });

            all.into_iter().map(|x| x.to).collect()
        };

        self.explicit_dependencies = self
            .explicit_dependencies
            .difference(&implicit_deps)
            .cloned()
            .collect();

        Ok(self)
    }

    fn item_id(&self) -> CardId {
        self.id
    }
}

#[derive(Serialize, Ord, PartialOrd, Eq, Hash, PartialEq, Debug, Clone)]
pub enum BackSide {
    Bool(bool),
    Text(TextData),
    Card(CardId),
    List(Vec<CardId>),
    Time(TimeStamp),
    Trivial, // Answer is obvious, used when card is more of a dependency anchor
    Invalid, // A reference card was deleted
}

#[derive(Serialize, Deserialize, Ord, PartialOrd, Eq, Hash, PartialEq, Debug, Clone)]
pub enum BarSide {
    Bool(bool),
    Text(String),
    Card(CardId),
    List(Vec<CardId>),
    Time(TimeStamp),
    Trivial,
    Invalid,
}

impl From<BarSide> for BackSide {
    fn from(value: BarSide) -> Self {
        match value {
            BarSide::Text(val) => BackSide::Text(val.into()),
            BarSide::Bool(val) => BackSide::Bool(val),
            BarSide::Card(val) => BackSide::Card(val),
            BarSide::List(val) => BackSide::List(val),
            BarSide::Time(val) => BackSide::Time(val),
            BarSide::Trivial => BackSide::Trivial,
            BarSide::Invalid => BackSide::Invalid,
        }
    }
}

impl Default for BackSide {
    fn default() -> Self {
        Self::Text(Default::default())
    }
}

impl From<String> for BackSide {
    fn from(s: String) -> Self {
        if let Ok(uuid) = Uuid::parse_str(&s) {
            Self::Card(uuid)
        } else if let Ok(timestamp) = TimeStamp::from_str(&s) {
            Self::Time(timestamp)
        } else if s.as_str() == Self::INVALID_STR {
            Self::Invalid
        } else {
            Self::Text(s.into())
        }
    }
}

impl BackSide {
    pub const INVALID_STR: &'static str = "__INVALID__";

    pub fn is_empty_text(&self) -> bool {
        if let Self::Text(s) = self {
            s.is_empty()
        } else {
            false
        }
    }

    pub fn is_time(&self) -> bool {
        matches!(self, Self::Time(_))
    }

    pub fn is_text(&self) -> bool {
        matches!(self, Self::Text(_))
    }

    pub fn is_ref(&self) -> bool {
        matches!(self, Self::Card(_))
    }

    pub fn as_timestamp(&self) -> Option<TimeStamp> {
        if let Self::Time(ts) = self {
            Some(ts.to_owned())
        } else {
            None
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        if let Self::Bool(b) = self {
            Some(*b)
        } else {
            None
        }
    }

    pub fn as_card(&self) -> Option<CardId> {
        if let Self::Card(card) = self {
            Some(*card)
        } else {
            None
        }
    }

    pub fn to_string(&self) -> String {
        match self {
            BackSide::Bool(b) => b.to_string(),
            BackSide::Text(s) => s.to_raw(),
            BackSide::Card(id) => id.to_string(),
            BackSide::List(ids) => format!("{ids:?}"),
            BackSide::Time(ts) => dbg!(ts.serialize()),
            BackSide::Trivial => "<trivial>".to_string(),
            BackSide::Invalid => "<invalid>".to_string(),
        }
    }

    pub fn dependencies(&self) -> BTreeSet<CardId> {
        let mut set = BTreeSet::default();
        match self {
            BackSide::Text(s) => {
                set.extend(s.card_ids());
            }
            BackSide::Card(card_id) => {
                let _ = set.insert(*card_id);
            }
            BackSide::List(vec) => {
                set.extend(vec.iter());
            }
            BackSide::Time(_) => {}
            BackSide::Trivial => {}
            BackSide::Invalid => {}
            BackSide::Bool(_) => {}
        }

        set
    }
}

impl<'de> Deserialize<'de> for BackSide {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;

        match value {
            Value::Array(arr) => {
                let mut ids = Vec::new();
                for item in arr {
                    if let Value::String(ref s) = item {
                        if let Ok(uuid) = Uuid::parse_str(s) {
                            ids.push(uuid);
                        } else {
                            return Err(serde::de::Error::custom("Invalid UUID in array"));
                        }
                    } else {
                        return Err(serde::de::Error::custom("Expected string in array"));
                    }
                }
                Ok(BackSide::List(ids))
            }
            Value::Bool(_) => Ok(BackSide::Trivial),
            Value::String(s) => Ok(s.into()),
            val => Ok(serde_json::from_value::<BarSide>(val).unwrap().into()),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Config;

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Copy, Eq, PartialEq, Hash, PartialOrd, Ord,
)]
#[serde(rename_all = "lowercase")]
pub enum CType {
    Instance,
    #[default]
    Normal,
    Unfinished,
    Attribute,
    Class,
    Statement,
    Event,
}

impl Display for CType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl CType {
    pub fn short_form(&self) -> &'static str {
        match self {
            CType::Instance => "I",
            CType::Normal => "N",
            CType::Attribute => "A",
            CType::Class => "C",
            CType::Unfinished => "U",
            CType::Statement => "S",
            CType::Event => "E",
        }
    }
}
