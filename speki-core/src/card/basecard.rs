use super::*;
use crate::{audio::AudioId, CardProperty, CardRefType};
use either::Either;
use indexmap::IndexSet;
use ledgerstore::{ItemReference, LedgerEvent, LedgerItem, PropertyCache, ReadLedger};
use omtrent::TimeStamp;
use serde::{Deserialize, Serialize, Serializer};
use std::{collections::BTreeSet, fmt::Display, str::FromStr};

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

    pub fn replace_card_id(&mut self, current: CardId, other: CardId) {
        for cmp in self.inner_mut() {
            match cmp {
                Either::Left(_) => {}
                Either::Right(link) => {
                    if link.id == current {
                        link.id = other;
                    }
                }
            }
        }
    }

    pub fn push_eval(&mut self, eval: EvalText) {
        let mut x: Vec<Either<String, TextLink>> = Default::default();

        for cmp in eval.components() {
            match &cmp.data {
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

    pub fn evaluate(&self, ledger: &impl ReadLedger<Item = RawCard>) -> String {
        let mut out = String::new();

        for cmp in &self.0 {
            match cmp {
                Either::Left(s) => out.push_str(&s),
                Either::Right(TextLink {
                    id: _,
                    alias: Some(alias),
                }) => {
                    out.push_str(alias);
                }
                Either::Right(TextLink { id, alias: None }) => match ledger
                    .load(*id)
                    .map(|card| card.name_eval(ledger).to_string())
                {
                    Some(name) => out.push_str(&name),
                    None => out.push_str("<invalid card ref>"),
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
        ledger: &impl ReadLedger<Item = RawCard>,
    ) -> Result<(), CardError> {
        match (self, back_side) {
            (AttrBackType::InstanceOfClass(instance), BackSide::Card(answer)) => {
                let answer_card = ledger.load(*answer).unwrap();
                let mut parent_class: CardId = match &answer_card.data {
                    CardType::Instance { class, .. } => *class,
                    _ => {
                        return Err(CardError::WrongCardType {
                            expected: CType::Instance,
                            actual: answer_card.data.fieldless(),
                        })
                    }
                };

                while parent_class != *instance {
                    let parent_card = ledger.load(parent_class).unwrap();
                    parent_class = match &parent_card.data {
                        CardType::Class {
                            parent_class: Some(class),
                            ..
                        } => *class,
                        CardType::Class {
                            parent_class: None, ..
                        } => {
                            return Err(CardError::InstanceOfNonClass {
                                actual_type: parent_card.data.fieldless(),
                            })
                        }
                        _ => {
                            return Err(CardError::WrongCardType {
                                expected: CType::Class,
                                actual: parent_card.data.fieldless(),
                            })
                        }
                    };
                }

                Ok(())
            }
            (AttrBackType::InstanceOfClass(_), _) => Err(CardError::AnswerMustBeCard {
                attribute_id: AttributeId::nil(),
            }),
            (AttrBackType::TimeStamp, BackSide::Time(_)) => Ok(()),
            (AttrBackType::TimeStamp, _) => Err(CardError::AnswerMustBeTime {
                attribute_id: AttributeId::nil(),
            }),
            (AttrBackType::Boolean, BackSide::Bool(_)) => Ok(()),
            (AttrBackType::Boolean, _) => Err(CardError::AnswerMustBeBool {
                attribute_id: AttributeId::nil(),
            }),
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
    /// Unique identifier for itself. Required because attribute cards are stored separately and need to refer to a specific attribute.
    pub id: AttributeId,
    /// The attribute itself. For example "birthdate" attribute on class person.
    pub pattern: String,
    /// An optional constraint on the answer to a given attribute. For example in the birthdate attribute, the answer must be a timestamp.
    pub back_type: Option<AttrBackType>,
}

pub enum BackSideConstraint {
    Card { ty: Option<CardId> },
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialOrd, Ord)]
pub struct ParamAnswer {
    pub answer: BackSide,
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialOrd, Ord)]
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
}

impl CardType {
    pub fn replace_card_id(&mut self, current: CardId, other: CardId) {
        match self {
            CardType::Instance {
                back,
                answered_params,
                name,
                class,
            } => {
                if let Some(back) = back {
                    back.replace_card_id(current, other);
                }

                name.replace_card_id(current, other);
                if class == &current {
                    *class = other;
                }

                for ans in answered_params.values_mut() {
                    ans.answer.replace_card_id(current, other);
                }
            }
            CardType::Normal { back, front } => {
                back.replace_card_id(current, other);
                front.replace_card_id(current, other);
            }
            CardType::Unfinished { front } => {
                front.replace_card_id(current, other);
            }
            CardType::Attribute {
                back,
                attribute: _,
                instance,
            } => {
                back.replace_card_id(current, other);
                if instance == &current {
                    *instance = other;
                }
            }
            CardType::Class {
                back,
                name,
                parent_class,
                default_question: _,
                ref mut attrs,
                params,
            } => {
                if let Some(back) = back {
                    back.replace_card_id(current, other);
                }
                name.replace_card_id(current, other);
                if let Some(parent_class) = parent_class {
                    if *parent_class == current {
                        *parent_class = other;
                    }
                }

                let updated_attrs: BTreeSet<Attrv2> = attrs
                    .iter()
                    .map(|attr| {
                        let mut attr = attr.clone();
                        if let Some(AttrBackType::InstanceOfClass(id)) = &mut attr.back_type {
                            if *id == current {
                                *id = other;
                            }
                        }
                        attr
                    })
                    .collect();

                *attrs = updated_attrs;

                let updated_params: BTreeMap<AttributeId, Attrv2> = params
                    .iter()
                    .map(|(attr_id, attr)| {
                        let mut attr = attr.clone();
                        if let Some(AttrBackType::InstanceOfClass(id)) = &mut attr.back_type {
                            if *id == current {
                                *id = other;
                            }
                        }
                        (*attr_id, attr)
                    })
                    .collect();

                *params = updated_params;
            }
            CardType::Statement { front } => {
                front.replace_card_id(current, other);
            }
        }
    }

    pub fn class(&self) -> Option<CardId> {
        match self {
            CardType::Instance { class, .. } => Some(*class),
            CardType::Normal { .. } => None,
            CardType::Unfinished { .. } => None,
            CardType::Attribute { .. } => None,
            CardType::Class { parent_class, .. } => *parent_class,
            CardType::Statement { .. } => None,
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
        }
    }

    pub fn name_textdata(&self, ledger: &impl ReadLedger<Item = RawCard>) -> TextData {
        match self {
            CardType::Instance { name, .. } => name.clone(),
            CardType::Normal { front, .. } => front.clone(),
            CardType::Unfinished { front, .. } => front.clone(),
            CardType::Attribute {
                attribute,
                instance,
                ..
            } => {
                let class: CardId = ledger
                    .get_property_cache(PropertyCache::new(
                        CardProperty::Attr,
                        attribute.to_string(),
                    ))
                    .into_iter()
                    .next()
                    .unwrap();

                let class = ledger.load(class).unwrap();
                let attr = class.get_attr(*attribute).unwrap();
                let instance = ledger.load(*instance).unwrap().data.name_textdata(ledger);
                let instance = instance.to_raw();

                let new = attr.pattern.replace("{}", &instance);

                TextData::from_raw(&new)
            }
            CardType::Class { name, .. } => name.clone(),
            CardType::Statement { front, .. } => front.clone(),
        }
    }

    pub fn param_to_ans(
        &self,
        ledger: &impl ReadLedger<Item = RawCard>,
    ) -> BTreeMap<Attrv2, Option<ParamAnswer>> {
        if let CardType::Instance {
            answered_params,
            class,
            ..
        } = self
        {
            let class = ledger.load(*class).unwrap();

            let mut params = class.params_on_class();
            params.extend(
                class
                    .params_on_parent_classes(ledger)
                    .values()
                    .flatten()
                    .cloned(),
            );

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

    pub fn type_name(&self) -> &str {
        match self {
            CardType::Unfinished { .. } => "unfinished",
            CardType::Statement { .. } => "statement",
            CardType::Attribute { .. } => "attribute",
            CardType::Instance { .. } => "instance",
            CardType::Normal { .. } => "normal",
            CardType::Class { .. } => "class",
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

#[derive(Serialize, Deserialize, Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct RawCard {
    pub id: Uuid,
    /// The context of which the name of the card makes sense. For example, instead of writing `kubernetes node`, you can just
    /// write `node` and put kubernetes as the namespace. This avoids unnecessarily long names for disambiguation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<CardId>,
    pub data: CardType,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub explicit_dependencies: BTreeSet<Uuid>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tags: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub front_audio: Option<AudioId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub back_audio: Option<AudioId>,
}

impl RawCard {
    /// Verifies that into_events correctly reproduce the same card.
    pub fn check_into_events(&self) -> Result<(), (Self, Self)> {
        let mut cloned = self.clone();
        cloned.front_audio = None;
        cloned.back_audio = None;

        if let CardType::Class {
            ref mut default_question,
            ..
        } = cloned.data
        {
            *default_question = None;
        }

        let events = self.clone().into_events();

        let mut new: RawCard = LedgerItem::new_default(self.id);
        for event in events {
            let LedgerEvent::ItemAction {
                id: _,
                action: LedgerAction::Modify(action),
            } = event
            else {
                panic!();
            };

            new = new.inner_run_action(action).unwrap();
        }

        if cloned != new {
            Err((cloned, new))
        } else {
            Ok(())
        }
    }

    pub fn into_events(self) -> Vec<CardEvent> {
        let id = self.id;
        let mut new = Self::new_default(id);
        let mut prev = new.clone();
        let actions = self.into_actions();

        let mut events: Vec<CardEvent> = vec![];
        for action in actions {
            new = new.inner_run_action(action.clone()).unwrap();
            if new != prev {
                let event = LedgerEvent::ItemAction {
                    id,
                    action: LedgerAction::Modify(action),
                };
                events.push(event);
            }

            prev = new.clone();
        }

        events
    }

    pub fn into_actions(self) -> Vec<CardAction> {
        let mut actions: Vec<CardAction> = vec![];

        let Self {
            id: _,
            namespace,
            data,
            explicit_dependencies,
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
                actions.push(CardAction::SetParamAnswers(answered_params));
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
                actions.push(CardAction::SetAttrs(attrs));
                actions.push(CardAction::SetParams(params.into_values().collect()));
            }
            CardType::Statement { front } => {
                actions.push(CardAction::StatementType { front });
            }
        }

        actions.push(CardAction::SetNamespace(namespace));

        for dep in explicit_dependencies {
            actions.push(CardAction::AddDependency(dep));
        }

        actions
    }

    pub fn parent_classes(&self, ledger: &impl ReadLedger<Item = RawCard>) -> IndexSet<CardId> {
        let key = match self.data {
            CardType::Instance { class, .. } => class,
            CardType::Class { .. } => self.id,
            _ => panic!(),
        };

        let expr = ItemExpr::Reference {
            items: Box::new(ItemExpr::Item(key)),
            ty: Some(CardRefType::ParentClass),
            reversed: false,
            recursive: true,
            include_self: false,
        };

        let mut classes = ledger.load_expr(expr);
        classes.insert(key);
        classes
    }

    pub fn params_on_parent_classes(
        &self,
        ledger: &impl ReadLedger<Item = RawCard>,
    ) -> BTreeMap<CardId, Vec<Attrv2>> {
        let mut parents = self.parent_classes(ledger);
        parents.shift_remove(&self.id);
        let mut out: BTreeMap<CardId, Vec<Attrv2>> = Default::default();

        for parent in parents {
            let params = ledger.load(parent).unwrap().params_on_class();
            out.insert(parent, params);
        }

        out
    }

    pub fn params_on_class(&self) -> Vec<Attrv2> {
        if let CardType::Class { params, .. } = &self.data {
            params.values().cloned().collect()
        } else {
            Default::default()
        }
    }

    pub fn param_answers(&self) -> BTreeMap<AttributeId, ParamAnswer> {
        if let CardType::Instance {
            answered_params, ..
        } = &self.data
        {
            answered_params.clone()
        } else {
            Default::default()
        }
    }

    pub fn frontside_eval(&self, ledger: &impl ReadLedger<Item = RawCard>) -> EvalText {
        DisplayData::new(ledger, self.namespace, &self.data, self.name_eval(ledger)).display(ledger)
    }

    pub fn name_eval(&self, ledger: &impl ReadLedger<Item = RawCard>) -> EvalText {
        EvalText::from_textdata(self.data.name_textdata(ledger), ledger)
    }

    pub fn backside_eval(&self, ledger: &impl ReadLedger<Item = RawCard>) -> EvalText {
        let from_back =
            |back: &BackSide| -> EvalText { EvalText::from_backside(back, ledger, true, false) };

        match &self.data {
            CardType::Instance { back, class, .. } => match back.as_ref() {
                Some(back) => from_back(back),
                None => EvalText::just_some_ref(*class, ledger),
            },
            CardType::Normal { back, .. } => from_back(back),
            CardType::Unfinished { .. } => {
                EvalText::just_some_string("<unfinished>".to_string(), ledger)
            }
            CardType::Attribute { back, .. } => from_back(back),
            CardType::Class {
                back, parent_class, ..
            } => match (back, parent_class) {
                (Some(theback), Some(pcl)) if theback.is_empty_text() => {
                    EvalText::just_some_string(
                        ledger
                            .load(*pcl)
                            .unwrap()
                            .data
                            .name_textdata(ledger)
                            .to_raw(),
                        ledger,
                    )
                }
                (None, Some(pcl)) => EvalText::just_some_ref(*pcl, ledger),
                (Some(back), _) => from_back(back),
                (_, _) => EvalText::default(),
            },
            CardType::Statement { .. } => {
                EvalText::just_some_string("<statement>".to_string(), ledger)
            }
        }
    }

    pub fn cache_front(&self, ledger: &impl ReadLedger<Item = RawCard>) -> String {
        match self.data.clone() {
            CardType::Instance { name, .. } => name.to_raw(),
            CardType::Normal { front, .. } => front.to_raw(),
            CardType::Unfinished { front } => front.to_raw(),
            CardType::Attribute {
                attribute: _,
                instance,
                ..
            } => {
                let attr = self.get_attr_rec(ledger).unwrap();

                let instance = ledger.load(instance).unwrap().data.name_fixed_ledger();
                let instance = instance.to_raw();

                let new = attr.pattern.replace("{}", &instance);
                new
            }
            CardType::Class { name, .. } => name.to_raw(),
            CardType::Statement { front } => front.to_raw(),
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
        }
    }

    pub fn attrs(&self) -> BTreeSet<Attrv2> {
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

    pub fn get_attr_rec(&self, ledger: &impl ReadLedger<Item = RawCard>) -> Option<Attrv2> {
        let CardType::Attribute {
            attribute,
            instance,
            ..
        } = &self.data
        else {
            return None;
        };

        let mut card: RawCard = ledger.load(*instance).unwrap();

        while let Some(parent) = card.parent_class() {
            card = ledger.load(parent).unwrap();
            if let Some(attr) = card.get_attr(*attribute) {
                return Some(attr);
            }
        }

        None
    }

    /// Get all attributes for this class including inherited ones from parent classes
    pub fn get_all_attributes(&self, ledger: &impl ReadLedger<Item = RawCard>) -> Vec<Attrv2> {
        if !self.data.is_class() {
            return vec![];
        }
        get_attributes(self.id, ledger)
    }

    fn similar_frontside(&self, ledger: &impl ReadLedger<Item = Self>) -> Vec<CardId> {
        let mut out = vec![];
        if true {
            return out;
        }
        let front = self.frontside_eval(ledger);

        for candidate in self.similar_names(ledger) {
            let candidate_front = ledger.load(candidate).unwrap().frontside_eval(ledger);
            if candidate_front.to_string().to_lowercase() == front.to_string().to_lowercase() {
                dbg!(&candidate_front, &front);
                out.push(candidate);
            }
        }

        out
    }

    fn similar_names(&self, ledger: &impl ReadLedger<Item = Self>) -> Vec<CardId> {
        let mut out = vec![];

        let mut inner: Vec<ItemExpr<Self>> = vec![];

        for prop in self.bigram_properties(ledger) {
            inner.push(ItemExpr::Property {
                property: prop.property,
                value: prop.value,
            });
        }

        let name = self.name_eval(ledger).to_string().to_lowercase();
        let expr = ItemExpr::Intersection(inner);
        for candidate in ledger.load_expr(expr) {
            if candidate == self.id {
                continue;
            }
            if ledger
                .load(candidate)
                .unwrap()
                .name_eval(ledger)
                .to_string()
                .to_lowercase()
                == name
            {
                out.push(candidate);
            }
        }

        out
    }

    fn bigram_properties(&self, ledger: &impl ReadLedger<Item = Self>) -> Vec<PropertyCache<Self>> {
        let resolved_text = self.name_eval(ledger);
        let mut out: Vec<PropertyCache<Self>> = vec![];

        for bigram in bigrams(&resolved_text) {
            let value = format!("{}{}", bigram[0], bigram[1]);
            let prop = PropertyCache {
                property: CardProperty::Bigram,
                value,
            };
            out.push(prop);
        }

        out
    }

    pub fn set_backside(mut self, new_back: BackSide) -> Self {
        let data = match self.data.clone() {
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

pub fn bigrams_expression_and(text: &str) -> ItemExpr<RawCard> {
    let bigrams = bigrams(text);
    let mut exprs: Vec<ItemExpr<RawCard>> = vec![];

    for bigram in bigrams {
        exprs.push(ItemExpr::Property {
            property: CardProperty::Bigram,
            value: format!("{}{}", bigram[0], bigram[1]),
        });
    }

    ItemExpr::Intersection(exprs)
}

/// Search for cards by text content using bigram matching.
/// Returns cards sorted by relevance (number of matching bigrams).
///
/// This matches the exact algorithm used in card_selector.rs.
///
/// # Arguments
/// * `normalized_search` - The normalized search string (already processed by normalize_string, includes ^ and $)
/// * `candidate_cards` - The set of cards to search within
/// * `ledger` - The ledger to query for property cache
/// * `limit` - Maximum number of results to return
pub fn search_cards_by_text(
    normalized_search: &str,
    candidate_cards: &indexmap::IndexSet<CardId>,
    ledger: &impl ledgerstore::ReadLedger<Item = RawCard>,
    limit: usize,
) -> Vec<(u32, CardId)> {
    use ledgerstore::PropertyCache;
    use std::collections::BTreeMap;

    debug_assert!(normalized_search.len() >= 2); // By default ^ and $ are added to search

    // If search is empty (just ^$), return all candidate cards
    if normalized_search.len() == 2 {
        return candidate_cards
            .iter()
            .take(limit)
            .enumerate()
            .map(|(idx, card)| (u32::MAX - idx as u32, *card))
            .collect();
    }

    let search_bigrams = bigrams(normalized_search);
    let mut matching_cards: BTreeMap<CardId, u32> = BTreeMap::new();

    // For each bigram, find cards that contain it
    for bigram in search_bigrams {
        let bigram_value = format!("{}{}", bigram[0], bigram[1]);
        let prop_cache = PropertyCache::new(CardProperty::Bigram, bigram_value);
        let matching_ids = ledger.get_property_cache(prop_cache);

        // Count how many bigrams match for each card
        for id in matching_ids {
            if candidate_cards.contains(&id) {
                *matching_cards.entry(id).or_insert(0) += 1;
            }
        }
    }

    // If we have few matches, add remaining candidates with score 0
    if matching_cards.len() < limit {
        for card in candidate_cards.iter().take(limit) {
            if !matching_cards.contains_key(card) {
                matching_cards.insert(*card, 0);
            }
        }
    }

    // Sort by match count (descending) and return
    let mut sorted_cards: Vec<_> = matching_cards.into_iter().collect();
    sorted_cards.sort_by(|a, b| b.1.cmp(&a.1));
    sorted_cards
        .into_iter()
        .map(|(id, score)| (score, id))
        .collect()
}

pub fn bigrams_expression_or(text: &str) -> ItemExpr<RawCard> {
    let bigrams = bigrams(text);
    let mut exprs: Vec<ItemExpr<RawCard>> = vec![];

    for bigram in bigrams {
        exprs.push(ItemExpr::Property {
            property: CardProperty::Bigram,
            value: format!("{}{}", bigram[0], bigram[1]),
        });
    }

    ItemExpr::Union(exprs)
}

pub fn bigrams(text: &str) -> Vec<[char; 2]> {
    normalize_string(text)
        .chars()
        .collect::<Vec<_>>()
        .windows(2)
        .map(|w| [w[0], w[1]])
        .collect()
}

pub fn normalize_string(s: &str) -> String {
    let s: String = deunicode::deunicode(s.trim())
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();

    format!("^{}$", s)
}

#[derive(Debug)]
pub enum CardError {
    MissingParam {
        param_id: AttributeId,
    },
    InstanceOfNonClass {
        actual_type: CType,
    },
    AttributeOfNonInstance,
    MissingAttribute {
        attribute_id: AttributeId,
    },
    DefaultQuestionNotClass,
    WrongCardType {
        expected: CType,
        actual: CType,
    },
    AnswerMustBeCard {
        attribute_id: AttributeId,
    },
    AnswerMustBeTime {
        attribute_id: AttributeId,
    },
    AnswerMustBeBool {
        attribute_id: AttributeId,
    },
    SubClassOfNonClass {
        parent_id: CardId,
    },
    BackTypeMustBeClass {
        back_type_id: CardId,
        actual_type: CType,
    },
    DuplicateAttribute {
        attribute_id: AttributeId,
    },
    DuplicateParam {
        param_id: AttributeId,
    },
    SimilarFront(CardId),
}

fn instance_is_of_type(
    instance: CardId,
    ty: CardId,
    ledger: &impl ReadLedger<Item = RawCard>,
) -> bool {
    let instance = ledger.load(instance).unwrap();
    assert!(instance.data.is_instance());

    get_parent_classes(ty, ledger)
        .into_iter()
        .find(|class| class.id == ty)
        .is_some()
}

fn get_parent_classes(class: CardId, ledger: &impl ReadLedger<Item = RawCard>) -> Vec<RawCard> {
    let class = ledger.load(class).unwrap();
    let mut classes: Vec<RawCard> = vec![class.clone()];
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

fn get_attributes(class: CardId, ledger: &impl ReadLedger<Item = RawCard>) -> Vec<Attrv2> {
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

    fn validate(&self, ledger: &impl ReadLedger<Item = Self>) -> Result<(), Self::Error> {
        if let Some(similar) = self.similar_frontside(ledger).into_iter().next() {
            return Err(CardError::SimilarFront(similar));
        }

        match &self.data {
            CardType::Instance {
                name: _,
                back: _,
                class,
                answered_params,
            } => {
                let mut class: Option<RawCard> = Some(ledger.load(*class).unwrap());
                let mut recursive_params: BTreeMap<AttributeId, Attrv2> = Default::default();

                while let Some(the_class) = class.clone() {
                    let CardType::Class {
                        params,
                        parent_class,
                        ..
                    } = &the_class.data
                    else {
                        return Err(CardError::InstanceOfNonClass {
                            actual_type: the_class.data.fieldless(),
                        });
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
                        None => return Err(CardError::MissingParam { param_id: *key_ans }),
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
                    let inst = ledger.load(*instance).unwrap();
                    return Err(CardError::InstanceOfNonClass {
                        actual_type: inst.data.fieldless(),
                    });
                };

                let class = {
                    let class = ledger.load(class).unwrap();
                    if !class.data.is_class() {
                        return Err(CardError::InstanceOfNonClass {
                            actual_type: class.data.fieldless(),
                        });
                    }
                    class
                };

                let Some(Attrv2 { back_type, .. }) = get_attributes(class.id, ledger)
                    .into_iter()
                    .find(|attr| attr.id == *attribute)
                else {
                    return Err(CardError::MissingAttribute {
                        attribute_id: *attribute,
                    });
                };

                match back_type {
                    Some(AttrBackType::Boolean) => {
                        if !matches!(attr_back, BackSide::Bool(_)) {
                            return Err(CardError::AnswerMustBeBool {
                                attribute_id: *attribute,
                            });
                        }
                    }
                    Some(AttrBackType::TimeStamp) => {
                        if !matches!(attr_back, BackSide::Time(_)) {
                            return Err(CardError::AnswerMustBeTime {
                                attribute_id: *attribute,
                            });
                        }
                    }
                    Some(AttrBackType::InstanceOfClass(back_class)) => {
                        if let BackSide::Card(answer) = attr_back {
                            if !instance_is_of_type(*answer, back_class, ledger) {
                                let answer_card = ledger.load(*answer).unwrap();
                                let expected_card = ledger.load(back_class).unwrap();
                                return Err(CardError::WrongCardType {
                                    expected: expected_card.data.fieldless(),
                                    actual: answer_card.data.fieldless(),
                                });
                            }
                        } else {
                            dbg!(name);
                            dbg!(&back);
                            dbg!(&attr_back);
                            dbg!(ledger.load(*instance));
                            dbg!(self);
                            let err = dbg!(CardError::AnswerMustBeCard {
                                attribute_id: *attribute
                            });
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
                params,
            } => {
                if let Some(parent) = parent_class {
                    if !ledger.load(*parent).unwrap().data.is_class() {
                        return Err(CardError::SubClassOfNonClass { parent_id: *parent });
                    }

                    // Collect all attribute IDs from parent classes
                    let mut parent_attr_ids = std::collections::BTreeSet::new();
                    let mut current_parent = Some(*parent);
                    while let Some(parent_id) = current_parent {
                        let parent_card = ledger.load(parent_id).unwrap();
                        if let CardType::Class {
                            attrs: parent_attrs,
                            parent_class: grandparent,
                            ..
                        } = &parent_card.data
                        {
                            for attr in parent_attrs {
                                parent_attr_ids.insert(attr.id);
                            }
                            current_parent = *grandparent;
                        } else {
                            break;
                        }
                    }

                    // Check that no attributes in this class duplicate parent attributes
                    for attr in attrs {
                        if parent_attr_ids.contains(&attr.id) {
                            return Err(CardError::DuplicateAttribute {
                                attribute_id: attr.id,
                            });
                        }
                    }

                    // Check that no params in this class duplicate parent params
                    let mut parent_param_ids = std::collections::BTreeSet::new();
                    let mut current_parent = Some(*parent);
                    while let Some(parent_id) = current_parent {
                        let parent_card = ledger.load(parent_id).unwrap();
                        if let CardType::Class {
                            params: parent_params,
                            parent_class: grandparent,
                            ..
                        } = &parent_card.data
                        {
                            for param in parent_params.values() {
                                parent_param_ids.insert(param.id);
                            }
                            current_parent = *grandparent;
                        } else {
                            break;
                        }
                    }

                    for param in params.values() {
                        if parent_param_ids.contains(&param.id) {
                            return Err(CardError::DuplicateParam { param_id: param.id });
                        }
                    }
                }

                for attr in attrs {
                    if let Some(AttrBackType::InstanceOfClass(back_type)) = attr.back_type {
                        if back_type == self.id {
                            continue;
                        }

                        let Some(back_card) = ledger.load(back_type) else {
                            dbg!("could not load backtype", back_type);
                            panic!();
                        };
                        if !back_card.data.is_class() {
                            return Err(CardError::BackTypeMustBeClass {
                                back_type_id: back_type,
                                actual_type: back_card.data.fieldless(),
                            });
                        }
                    }
                }
            }
            CardType::Statement { front: _ } => {}
        }

        Ok(())
    }

    fn ref_cache(&self) -> IndexSet<ItemReference<Self>> {
        let from = self.id;
        let mut out: IndexSet<ItemReference<Self>> = Default::default();

        fn refs_from_backside(from: CardId, back: &BackSide) -> IndexSet<ItemReference<RawCard>> {
            let mut out: IndexSet<ItemReference<RawCard>> = Default::default();
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
                BackSide::Bool(_) => {}
            }
            out
        }

        let Self {
            id: _,
            namespace,
            data,
            explicit_dependencies,
            tags: _,
            front_audio: _,
            back_audio: _,
        } = self;

        if let Some(ns) = namespace {
            out.insert(ItemReference::new(from, *ns, CardRefType::LinkRef));
        }

        for dep in explicit_dependencies {
            out.insert(ItemReference::new(
                from,
                *dep,
                CardRefType::ExplicitDependency,
            ));
        }

        match &data {
            CardType::Normal { front, back: _ } => {
                for id in front.card_ids() {
                    out.insert(ItemReference::new(from, id, CardRefType::LinkRef));
                }
            }
            CardType::Unfinished { front } => {
                for id in front.card_ids() {
                    out.insert(ItemReference::new(from, id, CardRefType::LinkRef));
                }
            }
            CardType::Instance {
                name,
                class,
                answered_params,
                back: _,
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
            CardType::Attribute {
                instance,
                attribute: _,
                back: _,
            } => {
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
                back: _,
                attrs,
                params,
            } => {
                for Attrv2 {
                    id: _,
                    pattern: _,
                    back_type,
                } in attrs
                {
                    match back_type {
                        Some(AttrBackType::InstanceOfClass(class_id)) => {
                            out.insert(ItemReference::new(from, *class_id, CardRefType::LinkRef));
                        }
                        None | Some(AttrBackType::Boolean | AttrBackType::TimeStamp) => {}
                    }
                }

                for Attrv2 {
                    id: _,
                    pattern: _,
                    back_type,
                } in params.values()
                {
                    match back_type {
                        Some(AttrBackType::InstanceOfClass(class_id)) => {
                            out.insert(ItemReference::new(from, *class_id, CardRefType::LinkRef));
                        }
                        None | Some(AttrBackType::Boolean | AttrBackType::TimeStamp) => {}
                    }
                }

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
            CardType::Statement { front } => {
                for id in front.card_ids() {
                    out.insert(ItemReference::new(from, id, CardRefType::LinkRef));
                }
            }
        };

        if let Some(back) = data.backside() {
            out.extend(refs_from_backside(from, back));
        }

        out
    }

    fn properties_cache(
        &self,
        ledger: &impl ReadLedger<Item = Self>,
    ) -> IndexSet<PropertyCache<Self>> {
        let mut out: IndexSet<PropertyCache<Self>> = Default::default();

        out.extend(self.bigram_properties(ledger));

        if self.data.backside().is_some_and(|b| !b.is_empty_text())
            && !matches!(&self.data, CardType::Unfinished { .. })
        {
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
            tags: Default::default(),
            explicit_dependencies: Default::default(),
            front_audio: Default::default(),
            back_audio: Default::default(),
        }
    }

    fn inner_run_action(mut self, action: CardAction) -> Result<Self, Self::Error> {
        match action {
            CardAction::InsertParam(param) => {
                let CardType::Class { ref mut params, .. } = &mut self.data else {
                    return Err(CardError::WrongCardType {
                        expected: CType::Class,
                        actual: self.data.fieldless(),
                    });
                };

                params.insert(param.id, param);
            }

            CardAction::SetBackTime(ts) => {
                self = self.set_backside(BackSide::Time(ts));
            }
            CardAction::SetParams(params) => {
                let new_params: BTreeMap<AttributeId, Attrv2> =
                    params.into_iter().map(|attr| (attr.id, attr)).collect();

                let CardType::Class { ref mut params, .. } = &mut self.data else {
                    return Err(CardError::WrongCardType {
                        expected: CType::Class,
                        actual: self.data.fieldless(),
                    });
                };

                *params = new_params;
            }
            CardAction::InsertParamAnswer { id, answer } => {
                let CardType::Instance {
                    ref mut answered_params,
                    ..
                } = &mut self.data
                else {
                    return Err(CardError::WrongCardType {
                        expected: CType::Instance,
                        actual: self.data.fieldless(),
                    });
                };

                answered_params.insert(id, answer);
            }
            CardAction::RemoveParamAnswer(id) => {
                let CardType::Instance {
                    ref mut answered_params,
                    ..
                } = &mut self.data
                else {
                    return Err(CardError::WrongCardType {
                        expected: CType::Instance,
                        actual: self.data.fieldless(),
                    });
                };

                answered_params.remove(&id).unwrap();
            }
            CardAction::SetParamAnswers(new_answered_params) => {
                let CardType::Instance {
                    ref mut answered_params,
                    ..
                } = &mut self.data
                else {
                    return Err(CardError::WrongCardType {
                        expected: CType::Instance,
                        actual: self.data.fieldless(),
                    });
                };

                *answered_params = new_answered_params;
            }
            CardAction::SetBackText(text) => {
                let backside = BackSide::Text(text);
                self = self.set_backside(backside);
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
            CardAction::ReplaceDependency { current, other } => {
                if self.explicit_dependencies.remove(&current) {
                    self.explicit_dependencies.insert(other);
                }
                self.data.replace_card_id(current, other);

                if self.namespace.is_some_and(|id| id == current) {
                    self.namespace = Some(other);
                }
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
                    debug_assert!(params.remove(&param).is_some());
                } else {
                    panic!("expeted class");
                }
            }
            CardAction::RemoveAttr(attr_id) => {
                if let CardType::Class { ref mut attrs, .. } = self.data {
                    let attr_len = attrs.len();
                    attrs.retain(|attr| attr.id != attr_id);
                    if attr_len == attrs.len() {
                        dbg!("attribute to remove not found");
                    }
                } else {
                    panic!("expected class");
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
                    attrs: Default::default(),
                    params: Default::default(),
                };
            }
            CardAction::UnfinishedType { front } => {
                self.data = CardType::Unfinished { front };
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
            },
            CardAction::SetAttrs(new_attrs) => {
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

#[derive(Deserialize, Serialize, Ord, PartialOrd, Eq, Hash, PartialEq, Debug, Clone)]
pub enum BackSide {
    Bool(bool),
    Text(TextData),
    Card(CardId),
    List(Vec<CardId>),
    Time(TimeStamp),
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
        } else {
            Self::Text(s.into())
        }
    }
}

impl BackSide {
    pub const INVALID_STR: &'static str = "__INVALID__";

    pub fn replace_card_id(&mut self, current: CardId, other: CardId) {
        match self {
            BackSide::Text(ref mut text) => {
                text.replace_card_id(current, other);
            }
            BackSide::Card(id) => {
                if *id == current {
                    *id = other;
                }
            }
            BackSide::List(ids) => {
                for id in ids.iter_mut() {
                    if *id == current {
                        *id = other;
                    }
                }
            }
            BackSide::Bool(_) => {}
            BackSide::Time(_) => {}
        }
    }

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
            BackSide::Bool(_) => {}
        }

        set
    }
}

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
}

impl Display for CType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl FromStr for CType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "instance" | "i" => Ok(CType::Instance),
            "normal" | "n" => Ok(CType::Normal),
            "unfinished" | "u" => Ok(CType::Unfinished),
            "attribute" | "a" => Ok(CType::Attribute),
            "class" | "c" => Ok(CType::Class),
            "statement" | "s" => Ok(CType::Statement),
            _ => Err(()),
        }
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
        }
    }
}
