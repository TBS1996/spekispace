use super::*;
use crate::{
    attribute::AttributeId, audio::AudioId, card_provider::CardProvider, CardProperty, RefType,
};
use either::Either;
use ledgerstore::{FixedLedger, LedgerItem};
use omtrent::TimeStamp;
use serde::{Deserialize, Serialize, Serializer};
use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    str::FromStr,
};

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

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn inner(&self) -> &Vec<Either<String, TextLink>> {
        &self.0
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

/// An attribute of a class is pre-made questions that can be asked about any of the classes' instances.
/// For example, all instances of `Person` can have the quesiton "when was {} born?"
///
/// Instances refer to both direct instances of the class, and instances of any sub-classes of the class.
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, Hash)]
pub struct Attrv2 {
    pub id: AttributeId,
    pub pattern: String,
    pub back_type: Option<CardId>,
}

pub enum BackSideConstraint {
    Card { ty: Option<CardId> },
}

/// Generic for a class. Any instance must define it.
pub struct Generic {
    id: Uuid,
    name: String,
    of_type: Option<CardId>,
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, Hash)]
pub enum CardType {
    /// A specific instance of a class
    /// For example, the instance might be Elvis Presley where the concept would be "Person"
    Instance {
        name: TextData,
        back: Option<BackSide>,
        class: CardId,
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
        back: Option<BackSide>,
        parent_class: Option<CardId>,
        default_question: Option<TextData>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attrs: Vec<Attrv2>,
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
            CardType::Instance { back, .. } => back.as_ref(),
            CardType::Normal { back, .. } => Some(back),
            CardType::Unfinished { .. } => None,
            CardType::Attribute { back, .. } => Some(back),
            CardType::Class { back, .. } => back.as_ref(),
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

    pub fn name_fixed_ledger(&self, provider: &FixedLedger<RawCard>) -> TextData {
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
                    .get_prop_cache(CardProperty::Attr, attribute.to_string())
                    .first()
                    .unwrap()
                    .parse()
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

    pub fn display_front(&self, provider: &CardProvider) -> TextData {
        match self {
            CardType::Instance {
                name, class, back, ..
            } => {
                let (class_name, default_question) =
                    match provider.providers.cards.load(class).map(|x| x.data) {
                        Some(CardType::Class {
                            default_question,
                            name,
                            ..
                        }) => (name, default_question),
                        None => {
                            dbg!(class);
                            panic!();
                        }
                        other => {
                            dbg!(class);
                            dbg!(other);
                            panic!();
                        }
                    };

                let thename = &name.evaluate(provider);
                let class_name = &class_name.evaluate(provider);

                match default_question {
                    Some(q) => {
                        let s = q.evaluate(provider).replace("{}", thename);
                        TextData::from_raw(&s)
                    }

                    None => {
                        if back.is_some() {
                            let s = format!("{thename} ({class_name})");
                            TextData::from_raw(&s)
                        } else {
                            name.clone()
                        }
                    }
                }
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
                    .get_prop_cache(CardProperty::Attr, attribute.to_string())
                    .first()
                    .unwrap()
                    .parse()
                    .unwrap();

                let class = provider.load(class).unwrap();

                let attr = class.get_attr(*attribute).unwrap();

                let new = attr.pattern.replace("{}", &format!("[[{instance}]]"));

                TextData::from_raw(&new)
            }
            CardType::Class {
                name, parent_class, ..
            } => match parent_class {
                Some(class) => {
                    let parent = provider.load(*class).unwrap().name_textdata();
                    let mut name = name.clone();
                    name.extend(parent.clone());
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

#[derive(Serialize, Deserialize, Debug, Clone, Hash)]
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
    pub fn cache_front(&self, ledger: &FixedLedger<RawCard>) -> String {
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

                let instance = ledger
                    .load(instance)
                    .unwrap()
                    .data
                    .name_fixed_ledger(ledger);
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

    pub fn mut_backside(&mut self) -> Option<&mut BackSide> {
        match &mut self.data {
            CardType::Instance { back, .. } => back.as_mut(),
            CardType::Normal { back, .. } => Some(back),
            CardType::Unfinished { .. } => None,
            CardType::Attribute { back, .. } => Some(back),
            CardType::Class { back, .. } => back.as_mut(),
            CardType::Statement { .. } => None,
            CardType::Event { .. } => todo!(),
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

    pub fn get_attr(&self, id: AttributeId) -> Option<Attrv2> {
        if let CardType::Class { ref attrs, .. } = &self.data {
            attrs.iter().find(|attr| attr.id == id).cloned()
        } else {
            None
        }
    }

    pub fn get_attr_rec(&self, ledger: FixedLedger<RawCard>) -> Option<Attrv2> {
        let CardType::Attribute {
            attribute,
            instance,
            ..
        } = &self.data
        else {
            return None;
        };

        let mut card: Self = ledger.load(*instance).unwrap();

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
            } => CardType::Instance {
                name,
                back: Some(new_back),
                class,
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
            } => CardType::Class {
                name,
                back: Some(new_back),
                parent_class,
                default_question,
                attrs,
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
fn resolve_text(txt: String, ledger: &FixedLedger<RawCard>, re: &Regex) -> String {
    let uuids: Vec<CardId> = re
        .find_iter(&txt)
        .filter_map(Result::ok)
        .map(|m| m.as_str().parse().unwrap())
        .collect();

    let mut s: String = re.replace_all(&txt, "").to_string();
    for id in uuids {
        let Some(card) = ledger.load(id) else {
            dbg!(&txt);
            dbg!(id);
            panic!();
            continue;
        };
        let txt = card.cache_front(ledger);
        s.push_str(&resolve_text(txt, ledger, re));
    }

    s
}

/// replaces all uuids on frontside of card with the frontside of the card referenced by uuid.
/// just appends it, doesn't preserve order, this is just to collect bigrams.
fn resolve_card(card: &RawCard, ledger: &FixedLedger<RawCard>) -> String {
    let uuid_regex = Regex::new(
        r"\b[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\b",
    )
    .unwrap();

    resolve_text(card.cache_front(ledger), ledger, &uuid_regex)
}

impl LedgerItem for RawCard {
    type Error = ();
    type Key = CardId;
    type RefType = RefType;
    type PropertyType = CardProperty;
    type Modifier = CardAction;

    fn ref_cache(&self) -> HashMap<Self::RefType, HashSet<CardId>> {
        let mut out: HashMap<Self::RefType, HashSet<Uuid>> = Default::default();

        if let Some(ns) = self.namespace {
            out.entry(RefType::LinkRef).or_default().insert(ns);
        }

        for dep in &self.explicit_dependencies {
            out.entry(RefType::ExplicitDependent)
                .or_default()
                .insert(*dep);
        }

        match &self.data {
            CardType::Normal { front, .. } => {
                for id in front.card_ids() {
                    out.entry(RefType::LinkRef).or_default().insert(id);
                }
            }
            CardType::Unfinished { front, .. } => {
                for id in front.card_ids() {
                    out.entry(RefType::LinkRef).or_default().insert(id);
                }
            }
            CardType::Instance { name, class, .. } => {
                for id in name.card_ids() {
                    out.entry(RefType::LinkRef).or_default().insert(id);
                }

                out.entry(RefType::Instance).or_default().insert(*class);
            }
            CardType::Attribute { instance, .. } => {
                // bruh
                // this allows us to, what? search for an instance, and get all the attribute cards for it? includingt his card? yeahhh
                out.entry(RefType::AttrClass).or_default().insert(*instance);
            }
            CardType::Class {
                name,
                default_question,
                parent_class,
                ..
            } => {
                for id in name.card_ids() {
                    out.entry(RefType::LinkRef).or_default().insert(id);
                }

                if let Some(def) = default_question {
                    for id in def.card_ids() {
                        out.entry(RefType::LinkRef).or_default().insert(id);
                    }
                }

                if let Some(class) = parent_class {
                    out.entry(RefType::SubClass).or_default().insert(*class);
                    out.entry(RefType::ExplicitDependent)
                        .or_default()
                        .insert(*class);
                }
            }
            CardType::Statement { front, .. } => {
                for id in front.card_ids() {
                    out.entry(RefType::LinkRef).or_default().insert(id);
                }
            }
            CardType::Event { front, .. } => {
                for id in front.card_ids() {
                    out.entry(RefType::LinkRef).or_default().insert(id);
                }
            }
        };

        if let Some(back) = &self.data.backside() {
            match back {
                BackSide::Text(txt) => {
                    for id in txt.card_ids() {
                        out.entry(RefType::LinkRef).or_default().insert(id);
                    }
                }
                BackSide::Card(id) => {
                    out.entry(RefType::LinkRef).or_default().insert(*id);
                }
                BackSide::List(ids) => {
                    for id in ids {
                        out.entry(RefType::LinkRef).or_default().insert(*id);
                    }
                }
                BackSide::Time(_) => {}
                BackSide::Trivial => {}
                BackSide::Invalid => {}
            }
        }

        out
    }

    fn properties_cache(&self, cache: &FixedLedger<Self>) -> HashSet<(Self::PropertyType, String)> {
        let mut out: HashSet<(Self::PropertyType, String)> = Default::default();

        let resolved_text = resolve_card(self, &cache);

        for bigram in bigrams(&resolved_text) {
            let value = format!("{}{}", bigram[0], bigram[1]);
            out.insert((CardProperty::Bigram, value));
        }

        match &self.data {
            CardType::Normal { .. } => {}
            CardType::Unfinished { .. } => {}
            CardType::Instance { .. } => {}
            CardType::Attribute { attribute, .. } => {
                out.insert((CardProperty::AttrId, attribute.to_string()));
            }
            CardType::Class { attrs, .. } => {
                for attr in attrs {
                    out.insert((CardProperty::Attr, attr.id.to_string()));
                }
            }
            CardType::Statement { .. } => {}
            CardType::Event { .. } => {}
        };

        let val = format!("{:?}", self.data.fieldless());
        out.insert((CardProperty::CardType, val));

        out
    }

    fn new_default(id: CardId) -> Self {
        Self {
            id,
            namespace: None,
            data: CardType::Normal {
                front: TextData::from_raw("uninit"),
                back: BackSide::Text("uninit".to_string().into()),
            },
            tags: Default::default(),
            explicit_dependencies: Default::default(),
            front_audio: Default::default(),
            back_audio: Default::default(),
        }
    }

    fn run_event(mut self, event: CardAction) -> Result<Self, ()> {
        match event {
            CardAction::SetDefaultQuestion(default) => match &mut self.data {
                CardType::Class {
                    ref mut default_question,
                    ..
                } => *default_question = default.map(|s| TextData::from_raw(&s)),
                _ => return Err(()),
            },
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
                    if !attrs.iter().any(|attr| attr.id == attrv2.id) {
                        attrs.push(attrv2);
                    }
                } else {
                    panic!("expeted class");
                }
            }
            CardAction::RemoveAttr(attr_id) => {
                if let CardType::Class { ref mut attrs, .. } = self.data {
                    let attr_len = attrs.len();
                    attrs.retain(|attr| attr.id != attr_id);
                    assert_eq!(attr_len - 1, attrs.len());
                } else {
                    panic!("expeted class");
                }
            }
        };

        let implicit_deps: BTreeSet<Uuid> = {
            let mut all = self.ref_cache();
            all.remove(&RefType::ExplicitDependent);
            all.into_values().flatten().collect()
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
    Text(TextData),
    Card(CardId),
    List(Vec<CardId>),
    Time(TimeStamp),
    Trivial, // Answer is obvious, used when card is more of a dependency anchor
    Invalid, // A reference card was deleted
}

#[derive(Serialize, Deserialize, Ord, PartialOrd, Eq, Hash, PartialEq, Debug, Clone)]
pub enum BarSide {
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

    pub fn invalidate_if_has_ref(&mut self, dep: CardId) {
        let has_ref = match self {
            BackSide::Card(card_id) => card_id == &dep,
            BackSide::List(vec) => vec.contains(&dep),
            BackSide::Text(_) => false,
            BackSide::Time(_) => false,
            BackSide::Trivial => false,
            BackSide::Invalid => false,
        };

        if has_ref {
            *self = Self::Invalid;
        }
    }

    pub fn is_text(&self) -> bool {
        matches!(self, Self::Text(_))
    }

    pub fn is_ref(&self) -> bool {
        matches!(self, Self::Card(_))
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
            BackSide::Text(s) => s.to_raw(),
            BackSide::Card(id) => id.to_string(),
            BackSide::List(ids) => format!("{ids:?}"),
            BackSide::Time(ts) => format!("{ts}"),
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
