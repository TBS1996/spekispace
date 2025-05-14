use super::*;
use crate::{
    attribute::AttributeId, audio::AudioId, card_provider::CardProvider, ledger::CardEvent,
    CardProperty, RefType,
};
use ledgerstore::LedgerItem;
use omtrent::TimeStamp;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub type CardId = Uuid;

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, Hash)]
pub enum CardType {
    /// A specific instance of a class
    /// For example, the instance might be Elvis Presley where the concept would be "Person"
    Instance {
        name: String,
        back: Option<BackSide>,
        class: CardId,
    },
    Normal {
        front: String,
        back: BackSide,
    },
    Unfinished {
        front: String,
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
        name: String,
        back: Option<BackSide>,
        parent_class: Option<CardId>,
        default_question: Option<String>,
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
        front: String,
    },
    /// gotta figure out if i want this to be a thing in itself or it can be handled with just attributes of an event class
    Event {
        front: String,
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
            CardType::Normal { back, .. } => Some(&back),
            CardType::Unfinished { .. } => None,
            CardType::Attribute { back, .. } => Some(&back),
            CardType::Class { back, .. } => back.as_ref(),
            CardType::Statement { .. } => None,
            CardType::Event { .. } => None,
        }
    }

    pub fn raw_front(&self) -> String {
        match self.clone() {
            CardType::Instance { name, .. } => name,
            CardType::Normal { front, .. } => front,
            CardType::Unfinished { front } => front,
            CardType::Attribute { .. } => format!("attr card"),
            CardType::Class { name, .. } => name,
            CardType::Statement { front } => front,
            CardType::Event { front, .. } => front,
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

    pub fn display_front(&self, provider: &CardProvider) -> String {
        match self {
            CardType::Instance {
                name, class, back, ..
            } => {
                let (class_name, default_question) = match provider
                    .providers
                    .cards
                    .load(class.to_string().as_str())
                    .unwrap()
                    .data
                {
                    CardType::Class {
                        default_question,
                        name,
                        ..
                    } => (name, default_question),
                    _ => panic!(),
                };

                match default_question {
                    Some(q) => q.replace("{}", &name),
                    None => {
                        if back.is_some() {
                            format!("{} ({})", name, class_name)
                        } else {
                            name.to_string()
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
                let attr = provider
                    .providers
                    .attrs
                    .load(attribute.to_string().as_str())
                    .unwrap();

                attr.name(*instance, provider.clone())
            }
            CardType::Class {
                name, parent_class, ..
            } => match parent_class {
                Some(class) => {
                    let parent = provider
                        .providers
                        .cards
                        .load(class.to_string().as_str())
                        .unwrap()
                        .data
                        .raw_front();
                    format!("{name} ({parent})")
                }
                None => name.to_string(),
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
    pub data: CardType,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub dependencies: BTreeSet<Uuid>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tags: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub front_audio: Option<AudioId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub back_audio: Option<AudioId>,
}

impl RawCard {
    // if a card is deleted that is being referenced we might have to change the card type
    pub fn remove_dep(&mut self, id: CardId) {
        todo!()
        /*
        if let Some(back) = self.data.mut_backside() {
            back.invalidate_if_has_ref(id);
        }

        match self {
            CardType::Instance{
                ref name,
                ref back,
                class,
            } => {
                if *class == id {
                    match back.clone() {
                        Some(backside) => {
                            *self = Self::Normal{
                                front: name.clone(),
                                back: backside,
                            }
                        }
                        None => {
                            *self = Self::Unfinished{
                                front: name.clone(),
                            }
                        }
                    }
                }
            }
            CardType::Normal{..} => {}
            CardType::Unfinished{..} => {}
            CardType::Attribute{..} => {}
            CardType::Class{
                name,
                back,
                parent_class,
            } => {
                if *parent_class == Some(id) {
                    *self = Self::Class{
                        name: name.clone(),
                        back: back.clone(),
                        parent_class: None,
                    };
                }
            }
            CardType::Statement{..} => {}
            CardType::Event{..} => {}
        };
        */
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

    /// Returns all dependencies of the card
    pub fn dependencies(&self) -> BTreeSet<CardId> {
        let mut deps = self.dependencies.clone();
        if let Some(back) = self.ref_backside() {
            deps.extend(back.dependencies());
        }

        match &self.data {
            CardType::Instance { class, .. } => {
                deps.insert(*class);
            }
            CardType::Normal { .. } => {}
            CardType::Unfinished { .. } => {}
            CardType::Attribute { instance, .. } => {
                deps.insert(*instance);
            }
            CardType::Class { parent_class, .. } => {
                if let Some(class) = parent_class {
                    deps.insert(*class);
                }
            }
            CardType::Statement { .. } => {}
            CardType::Event { .. } => {}
        }

        deps
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
            } => CardType::Class {
                name,
                back: Some(new_back),
                parent_class,
                default_question,
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

impl LedgerItem<CardEvent> for RawCard {
    type Error = ();
    type RefType = RefType;
    type PropertyType = CardProperty;

    fn ref_cache(&self) -> HashMap<Self::RefType, HashSet<CardId>> {
        let mut out: HashMap<Self::RefType, HashSet<Uuid>> = Default::default();

        for dep in &self.dependencies {
            out.entry(RefType::Dependent).or_default().insert(*dep);
        }

        match &self.data {
            CardType::Normal { .. } => {}
            CardType::Unfinished { .. } => {}
            CardType::Instance { class, .. } => {
                out.entry(RefType::Instance).or_default().insert(*class);
                out.entry(RefType::Dependent).or_default().insert(*class);
            }
            CardType::Attribute { instance, .. } => {
                out.entry(RefType::AttrClass).or_default().insert(*instance);
                out.entry(RefType::Dependent).or_default().insert(*instance);
            }
            CardType::Class { parent_class, .. } => {
                if let Some(class) = parent_class {
                    out.entry(RefType::SubClass).or_default().insert(*class);
                    out.entry(RefType::Dependent).or_default().insert(*class);
                }
            }
            CardType::Statement { .. } => {}
            CardType::Event { .. } => {}
        };

        if let Some(back) = &self.data.backside() {
            match back {
                BackSide::Text(_) => {}
                BackSide::Card(id) => {
                    out.entry(RefType::BackRef).or_default().insert(*id);
                    out.entry(RefType::Dependent).or_default().insert(*id);
                }
                BackSide::List(ids) => {
                    for id in ids {
                        out.entry(RefType::BackRef).or_default().insert(*id);
                        out.entry(RefType::Dependent).or_default().insert(*id);
                    }
                }
                BackSide::Time(_) => {}
                BackSide::Trivial => {}
                BackSide::Invalid => {}
            }
        }

        out
    }

    fn properties_cache(&self) -> HashSet<(Self::PropertyType, String)> {
        let mut out: HashSet<(Self::PropertyType, String)> = Default::default();

        for bigram in bigrams(&self.data.raw_front()) {
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
            CardType::Class { .. } => {}
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
            data: CardType::Normal {
                front: "uninit".to_string(),
                back: BackSide::Text("uninit".to_string()),
            },
            tags: Default::default(),
            dependencies: Default::default(),
            front_audio: Default::default(),
            back_audio: Default::default(),
        }
    }

    fn run_event(mut self, event: CardEvent) -> Result<Self, ()> {
        for action in event.action {
            match action {
                CardAction::SetDefaultQuestion(default) => match &mut self.data {
                    CardType::Class {
                        ref mut default_question,
                        ..
                    } => *default_question = default,
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
                CardAction::DeleteCard => {}
                CardAction::AddDependency(dependency) => {
                    self.dependencies.insert(dependency);
                }
                CardAction::RemoveDependency(dependency) => {
                    self.dependencies.remove(&dependency);
                    self.remove_dep(dependency);
                }
                CardAction::SetBackRef(reff) => {
                    let backside = BackSide::Card(reff);
                    self = self.set_backside(backside);
                }
            }
        }

        Ok(self)
    }

    fn item_id(&self) -> CardId {
        self.id
    }
}

#[derive(Serialize, Ord, PartialOrd, Eq, Hash, PartialEq, Debug, Clone)]
pub enum BackSide {
    Text(String),
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
            BarSide::Text(val) => BackSide::Text(val),
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
        } else if let Some(timestamp) = TimeStamp::from_string(s.clone()) {
            Self::Time(timestamp)
        } else if s.as_str() == Self::INVALID_STR {
            Self::Invalid
        } else {
            Self::Text(s)
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
            BackSide::Text(s) => s.clone(),
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
            BackSide::Text(_) => {}
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

#[derive(Serialize, Deserialize, Debug, Clone, Default, Copy, Eq, PartialEq, Hash, PartialOrd)]
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

impl CType {
    pub fn supports_backside(&self) -> bool {
        match self {
            CType::Instance => true,
            CType::Normal => true,
            CType::Class => true,
            CType::Attribute => true,
            CType::Unfinished => false,
            CType::Statement => false,
            CType::Event => false,
        }
    }
}
