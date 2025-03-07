use super::*;
use crate::{
    attribute::AttributeId, audio::AudioId, card_provider::CardProvider, ledger::CardEvent, App,
    Attribute, CacheKey, DepCacheKey,
};
use omtrent::TimeStamp;
use serde::{Deserialize, Serialize};
use speki_dto::LedgerItem;
use std::collections::{HashMap, HashSet};

pub type CardId = Uuid;

/// Represents the card without userdata, the part that can be freely shared among different users.
#[derive(Clone, Debug, Hash, PartialEq)]
pub struct BaseCard {
    pub id: CardId,
    pub ty: CardType,
    pub dependencies: BTreeSet<CardId>,
    pub front_audio: Option<AudioId>,
    pub back_audio: Option<AudioId>,
}

impl BaseCard {
    pub fn new(ty: impl Into<CardType>) -> Self {
        Self::new_with_id(CardId::new_v4(), ty)
    }

    pub fn new_with_id(id: impl Into<Option<CardId>>, ty: impl Into<CardType>) -> Self {
        let id: Option<CardId> = id.into();
        let id = id.unwrap_or_else(|| CardId::new_v4());

        Self {
            id,
            ty: ty.into(),
            dependencies: Default::default(),
            front_audio: None,
            back_audio: None,
        }
    }
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, Hash)]
pub enum CardType {
    /// A specific instance of a class
    /// For example, the instance might be Elvis Presley where the concept would be "Person"
    /// the right answer is to know which class the instance belongs to
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
        back: BackSide,
        parent_class: Option<CardId>,
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
    Event {
        front: String,
        start_time: TimeStamp,
        end_time: Option<TimeStamp>,
        parent_event: Option<CardId>,
    },
}

impl CardType {
    pub fn class(&self) -> Option<CardId> {
        from_any(self.clone()).class()
    }

    pub fn raw_front(&self) -> String {
        from_any(self.clone()).front.unwrap_or_default()
    }

    pub fn raw_back(&self) -> String {
        from_any(self.clone())
            .back
            .map(|b| b.to_string())
            .unwrap_or_default()
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
                dependencies.extend(back.dependencies().iter());
                if let Some(id) = parent_class {
                    dependencies.insert(*id);
                }
                dependencies
            }
            CardType::Statement { .. } => Default::default(),
            CardType::Event { .. } => todo!(),
        }
    }

    pub async fn display_front(&self, provider: &CardProvider) -> String {
        match self {
            CardType::Instance { name, .. } => name.clone(),
            CardType::Normal { front, .. } => front.clone(),
            CardType::Unfinished { front, .. } => front.clone(),
            CardType::Attribute { .. } => "oops".to_string(),
            CardType::Class { name, .. } => name.clone(),
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

#[derive(Serialize, Deserialize, Default, Debug, Clone, Hash)]
struct RawType {
    pub ty: CType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub front: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    back: Option<BackSide>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    class: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    instance: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    attribute: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    start_time: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    end_time: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    parent_event: Option<Uuid>,
}

impl RawType {
    pub fn class(&self) -> Option<Uuid> {
        self.class.clone()
    }

    pub fn backside(&self) -> Option<BackSide> {
        use CType as C;
        match self.ty {
            C::Instance | C::Normal | C::Attribute | C::Class => self.back.clone(),
            C::Unfinished | C::Statement | C::Event => None,
        }
    }

    pub fn mut_backside(&mut self) -> Option<&mut BackSide> {
        use CType as C;
        match self.ty {
            C::Instance | C::Normal | C::Attribute | C::Class => self.back.as_mut(),
            C::Unfinished | C::Statement | C::Event => None,
        }
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
            CardType::Instance {class, ..} => Some(class),
            CardType::Normal {..} => None,
            CardType::Unfinished {..} => None,
            CardType::Attribute {..} => None,
            CardType::Class {parent_class, ..} => parent_class,
            CardType::Statement { .. } => None,
            CardType::Event {..} => None,
        }
    }


    pub fn mut_backside(&mut self) -> Option<&mut BackSide> {
        match &mut self.data {
            CardType::Instance { back, ..} => back.as_mut(),
            CardType::Normal { back, .. } => Some(back),
            CardType::Unfinished { .. } => None,
            CardType::Attribute { back, .. } => Some(back),
            CardType::Class { back, ..} => Some(back),
            CardType::Statement { .. } => None,
            CardType::Event { .. } => todo!(),
        }
    }

    pub fn ref_backside(&self) -> Option<&BackSide> {
        match &self.data {
            CardType::Instance { back, ..} => back.as_ref(),
            CardType::Normal { back, .. } => Some(back),
            CardType::Unfinished { .. } => None,
            CardType::Attribute { back, .. } => Some(back),
            CardType::Class { back, ..} => Some(back),
            CardType::Statement { .. } => None,
            CardType::Event { .. } => todo!(),
        }
    }

    /// Returns all dependencies of the card
    pub async fn dependencies(&self) -> BTreeSet<CardId> {
        let mut deps = self.dependencies.clone();
        if let Some(back) = self.ref_backside() {
            deps.extend(back.dependencies());
        }

        match &self.data {
            CardType::Instance { class, ..} => {
                deps.insert(*class);
            },
            CardType::Normal { .. } => {},
            CardType::Unfinished { .. } => {},
            CardType::Attribute {instance, ..} => {
                deps.insert(*instance);
            },
            CardType::Class { parent_class, .. } =>  {
                if let Some(class) = parent_class {
                    deps.insert(*class);
                }
            },
            CardType::Statement { .. } => {},
            CardType::Event { .. } => {},
        }

        deps
    }

    pub fn set_backside(mut self, new_back: BackSide) -> Self {
        let data = match self.data.clone() {
            x @ CardType::Event{..} => x,
            CardType::Instance{ name, back: _, class } => CardType::Instance { name , back: Some(new_back), class},
            x @ CardType::Statement{..} => x,

            CardType::Normal{ front, back: _ } => CardType::Normal {
                front,
                back: new_back,
            },
            CardType::Unfinished{ front } => CardType::Normal {
                front,
                back: new_back,
            },
            CardType::Attribute{
                attribute,
                instance: concept_card,
                back: _
            } => CardType::Attribute {
                attribute,
                back: new_back,
                instance: concept_card,
            },
            CardType::Class { name, back: _, parent_class } => CardType::Class { name, back: new_back, parent_class  },
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

    fn dep_cache(&self) -> HashMap<&'static str, HashSet<CardId>> {
        let mut out: HashMap<&str, HashSet<Uuid>> = Default::default();

        for dep in &self.dependencies {
            out.entry(&DepCacheKey::Dependent.to_str())
                .or_default()
                .insert(*dep);
        }

        match &self.data.ty {
            CType::Normal => {}
            CType::Unfinished => {}
            CType::Instance => {
                let class = self.data.class.unwrap();
                out.entry(&DepCacheKey::Instance.to_str())
                    .or_default()
                    .insert(class);
            }
            CType::Attribute => {
                let instance = self.data.instance.unwrap();
                out.entry(&DepCacheKey::AttrClass.to_str())
                    .or_default()
                    .insert(instance);
            }
            CType::Class => {
                if let Some(class) = self.data.class {
                    out.entry(&DepCacheKey::SubClass.to_str())
                        .or_default()
                        .insert(class);
                }
            }
            CType::Statement => {}
            CType::Event => {}
        };

        if let Some(back) = &self.data.back {
            match back {
                BackSide::Text(_) => {}
                BackSide::Card(id) => {
                    out.entry(&DepCacheKey::BackRef.to_str())
                        .or_default()
                        .insert(*id);
                }
                BackSide::List(ids) => {
                    for id in ids {
                        out.entry(&DepCacheKey::BackRef.to_str())
                            .or_default()
                            .insert(*id);
                    }
                }
                BackSide::Time(_) => {}
                BackSide::Trivial => {}
                BackSide::Invalid => {}
            }
        }

        out
    }

    fn caches(&self) -> HashSet<(&'static str, String)> {
        let mut out: HashSet<(&'static str, String)> = Default::default();

        for bigram in bigrams(&self.data.front.clone().unwrap_or_default()) {
            out.insert(CacheKey::Bigram(bigram).to_parts());
        }

        match &self.data.ty {
            CType::Normal => {}
            CType::Unfinished => {}
            CType::Instance => {}
            CType::Attribute => {
                out.insert(CacheKey::AttrId(self.data.attribute.unwrap()).to_parts());
            }
            CType::Class => {}
            CType::Statement => {}
            CType::Event => {}
        };

        out.insert(CacheKey::CardType(self.data.ty).to_parts());

        out
    }

    fn new_default(id: CardId) -> Self {
        Self {
            id,
            data: from_any(CardType::Normal {
                front: "uninit".to_string(),
                back: BackSide::Text("uninit".to_string()),
            }),
            tags: Default::default(),
            dependencies: Default::default(),
            front_audio: Default::default(),
            back_audio: Default::default(),
        }
    }

    fn run_event(mut self, event: CardEvent) -> Result<Self, ()> {
        for action in event.action {
            match action {
                CardAction::SetFrontAudio(audio) => {
                    self.front_audio = audio;
                }
                CardAction::SetBackAudio(audio) => {
                    self.back_audio = audio;
                }
                CardAction::UpsertCard(ty) => {
                    self.data = from_any(ty);
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

    fn derive_events(&self) -> Vec<CardEvent> {
        todo!();
        let mut actions = vec![];

        //let action = CardAction::UpsertCard (self.data.ty.clone() );
        //actions.push(action);

        if let Some(audio) = self.front_audio {
            let action = CardAction::SetFrontAudio(Some(audio));
            actions.push(action);
        }

        if let Some(audio) = self.back_audio {
            let action = CardAction::SetFrontAudio(Some(audio));
            actions.push(action);
        }

        for dep in &self.dependencies {
            let action = CardAction::AddDependency(*dep);
            actions.push(action);
        }

        let id = self.id;

        actions
            .into_iter()
            .map(|action| CardEvent::new(id, action))
            .collect()
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
        let mut s = serde_json::to_string(self).unwrap();
        s.remove(0);
        s.pop();
        s
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
            _ => Err(serde::de::Error::custom("Expected a string or an array")),
        }
    }
}

/*
impl Serialize for BackSide {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match *self {
            BackSide::Trivial => serializer.serialize_bool(false),
            BackSide::Invalid => serializer.serialize_str(Self::INVALID_STR),
            BackSide::Time(ref t) => serializer.serialize_str(&t.serialize()),
            BackSide::Text(ref s) => serializer.serialize_str(s),
            BackSide::Card(ref id) => serializer.serialize_str(&id.to_string()),
            BackSide::List(ref ids) => {
                let mut seq = serializer.serialize_seq(Some(ids.len()))?;
                for id in ids {
                    seq.serialize_element(&id.to_string())?;
                }
                seq.end()
            }
        }
    }
}
*/

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

fn into_any(raw: RawType) -> CardType {
    match raw.ty {
        CType::Instance => CardType::Instance {
            name: raw.front.unwrap(),
            class: raw.class.unwrap(),
            back: raw.back,
        },
        CType::Normal => CardType::Normal {
            front: raw.front.unwrap(),
            back: raw.back.unwrap(),
        },
        CType::Unfinished => CardType::Unfinished {
            front: raw.front.unwrap(),
        },
        CType::Attribute => CardType::Attribute {
            attribute: raw.attribute.unwrap(),
            back: raw.back.unwrap(),
            instance: raw.instance.unwrap(),
        },
        CType::Class => CardType::Class {
            name: raw.front.unwrap(),
            back: raw.back.unwrap(),
            parent_class: raw.class,
        },
        CType::Statement => CardType::Statement {
            front: raw.front.unwrap(),
        },
        CType::Event => CardType::Event {
            front: raw.front.unwrap(),
            start_time: raw
                .start_time
                .clone()
                .map(TimeStamp::from_string)
                .flatten()
                .unwrap_or_default(),
            end_time: raw.end_time.clone().map(TimeStamp::from_string).flatten(),
            parent_event: raw.parent_event,
        },
    }
}

pub fn from_any(ty: CardType) -> RawType {
    let mut raw = RawType::default();
    let fieldless = ty.fieldless();
    raw.ty = fieldless;

    match ty {
        CardType::Instance { name, class, back } => {
            raw.class = Some(class);
            raw.front = Some(name);
            raw.back = back;
        }
        CardType::Normal { front, back } => {
            raw.front = Some(front);
            raw.back = Some(back);
        }
        CardType::Unfinished { front } => {
            raw.front = Some(front);
        }
        CardType::Attribute {
            attribute,
            back,
            instance,
        } => {
            raw.attribute = Some(attribute);
            raw.back = Some(back);
            raw.instance = Some(instance);
        }
        CardType::Class {
            name,
            back,
            parent_class,
        } => {
            raw.front = Some(name);
            raw.back = Some(back);
            raw.class = parent_class;
        }
        CardType::Statement { front } => {
            raw.front = Some(front);
        }
        CardType::Event {
            front,
            start_time,
            end_time,
            parent_event,
        } => {
            raw.front = Some(front);
            raw.start_time = Some(start_time.serialize());
            raw.end_time = end_time.map(|t| t.serialize());
            raw.parent_event = parent_event;
        }
    };

    raw
}
