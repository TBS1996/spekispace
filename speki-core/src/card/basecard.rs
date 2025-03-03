use std::collections::HashSet;
use omtrent::TimeStamp;
use speki_dto::{RunLedger};
use serde::{Deserialize, Serialize};
use super::*;
use crate::{attribute::AttributeId, audio::AudioId, card_provider::CardProvider, ledger::CardEvent, App, Attribute, CacheKey};

pub type CardId = Uuid;

/// Represents the card without userdata, the part that can be freely shared among different users.
#[derive(Clone, Serialize, Deserialize, Debug, Hash, PartialEq)]
#[serde(from = "RawCard", into = "RawCard")]
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


    pub fn back_side(&self) -> Option<&BackSide> {
        match &self.ty {
            CardType::Instance(instance) => instance.back.as_ref(),
            CardType::Attribute(card) => Some(&card.back),
            CardType::Normal(card) => Some(&card.back),
            CardType::Class(card) => Some(&card.back),
            CardType::Unfinished(_) => None?,
            CardType::Statement(_) => None?,
            CardType::Event(_) => None?,
        }
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

    /// Returns all dependencies of the card
    pub async fn dependencies(&self) -> BTreeSet<CardId> {
        let mut deps = self.dependencies.clone();
        deps.extend(self.ty.get_dependencies().await);
        deps
    }
}

impl From<RawCard> for BaseCard {
    fn from(raw: RawCard) -> Self {
        Self {
            id: raw.id,
            ty: into_any(raw.data),
            dependencies: raw.dependencies,
            front_audio: raw.front_audio,
            back_audio: raw.back_audio,
        }
    }
}

impl From<BaseCard> for RawCard {
    fn from(card: BaseCard) -> Self {
        RawCard {
            id: card.id,
            data: from_any(card.ty),
            dependencies: card.dependencies,
            tags: Default::default(),
            front_audio: card.front_audio,
            back_audio: card.back_audio,
        }
    }
}

#[async_trait::async_trait(?Send)]
impl CardTrait for NormalCard {
    async fn get_dependencies(&self) -> BTreeSet<CardId> {
        let mut set: BTreeSet<CardId> = Default::default();
        set.extend(self.back.dependencies().iter());
        set
    }
}

#[async_trait::async_trait(?Send)]
impl CardTrait for InstanceCard {
    async fn get_dependencies(&self) -> BTreeSet<CardId> {
        let mut set = BTreeSet::default();
        set.insert(self.class);
        set
    }
}

#[async_trait::async_trait(?Send)]
impl CardTrait for AttributeCard {
    async fn get_dependencies(&self) -> BTreeSet<CardId> {
        let mut dependencies = BTreeSet::default();
        dependencies.insert(self.instance);
        dependencies.extend(self.back.dependencies().iter());
        dependencies
    }
}

#[async_trait::async_trait(?Send)]
impl CardTrait for UnfinishedCard {
    async fn get_dependencies(&self) -> BTreeSet<CardId> {
        Default::default()
    }
}

impl From<StatementCard> for CardType {
    fn from(value: StatementCard) -> Self {
        Self::Statement(value)
    }
}

impl From<NormalCard> for CardType {
    fn from(value: NormalCard) -> Self {
        Self::Normal(value)
    }
}
impl From<UnfinishedCard> for CardType {
    fn from(value: UnfinishedCard) -> Self {
        Self::Unfinished(value)
    }
}
impl From<AttributeCard> for CardType {
    fn from(value: AttributeCard) -> Self {
        Self::Attribute(value)
    }
}
impl From<InstanceCard> for CardType {
    fn from(value: InstanceCard) -> Self {
        Self::Instance(value)
    }
}
impl From<ClassCard> for CardType {
    fn from(value: ClassCard) -> Self {
        Self::Class(value)
    }
}

#[async_trait::async_trait(?Send)]
impl CardTrait for ClassCard {
    async fn get_dependencies(&self) -> BTreeSet<CardId> {
        let mut dependencies: BTreeSet<CardId> = Default::default();
        dependencies.extend(self.back.dependencies().iter());
        if let Some(id) = self.parent_class {
            dependencies.insert(id);
        }
        dependencies
    }
}

/// An unfinished card
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, Hash)]
pub struct UnfinishedCard {
    pub front: String,
}

/// Just a normal flashcard
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, Hash)]
pub struct NormalCard {
    pub front: String,
    pub back: BackSide,
}

/// A class, which is something that has specific instances of it, but is not a single thing in itself.
/// A class might also have sub-classes, for example, the class chemical element has a sub-class isotope
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, Hash)]
pub struct ClassCard {
    pub name: String,
    pub back: BackSide,
    pub parent_class: Option<CardId>,
}

/// An attribute describes a specific instance of a class. For example the class Person can have attribute "when was {} born?"
/// this will be applied to all instances of the class and its subclasses
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, Hash)]
pub struct AttributeCard {
    pub attribute: AttributeId,
    pub back: BackSide,
    pub instance: CardId,
}

impl AttributeCard {
    pub async fn display_front(&self, provider: &CardProvider) -> String {
        "omg an attribute".to_string()
    }
     /* 
     let attr =    provider
            .providers
            .attrs
            .load_item(self.attribute)
            .await
            .map(|dto| Attribute::from_dto(dto, provider.clone()));

        let Some(attr) = attr else {
            return format!("attribute not found ? {}", self.attribute);
        };

            attr
            .name(self.instance)
            .await
            .unwrap_or_else(|| "oops, instance is deleted".to_string())
    }
    */
}

/// A specific instance of a class
/// For example, the instance might be Elvis Presley where the concept would be "Person"
/// the right answer is to know which class the instance belongs to
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, Hash)]
pub struct InstanceCard {
    pub name: String,
    pub back: Option<BackSide>,
    pub class: CardId,
}

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
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, Hash)]
pub struct StatementCard {
    pub front: String,
}

#[async_trait::async_trait(?Send)]
impl CardTrait for StatementCard {
    async fn get_dependencies(&self) -> BTreeSet<CardId> {
        Default::default()
    }
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, Hash)]
pub struct EventCard {
    pub front: String,
    pub start_time: TimeStamp,
    pub end_time: Option<TimeStamp>,
    pub parent_event: Option<CardId>,
}

impl EventCard {
    /// Returns `true` if the other event fits within self-event's timeline
    fn inner_valid_sub_event(&self, other: &Self) -> bool {
        if other.start_time < self.start_time {
            false
        } else if let (Some(self_end), Some(other_end)) = (&self.end_time, &other.end_time) {
            self_end > other_end
        } else {
            true
        }
    }

    pub async fn valid_sub_event(&self, other: CardId, app: &App) -> bool {
        let other = app.load_card(other).await.unwrap();

        let CardType::Event(other) = other.base.ty else {
            panic!("wrong type");
        };

        self.inner_valid_sub_event(&other)
    }

    pub async fn valid_parent_event(&self, parent: CardId, app: &App) -> bool {
        let parent = app.load_card(parent).await.unwrap();
        let CardType::Event(parent) = parent.base.ty else {
            panic!("wrong type");
        };

        parent.inner_valid_sub_event(self)
    }
}

impl From<EventCard> for CardType {
    fn from(value: EventCard) -> Self {
        Self::Event(value)
    }
}

#[async_trait::async_trait(?Send)]
impl CardTrait for EventCard {
    async fn get_dependencies(&self) -> BTreeSet<CardId> {
        let mut set: BTreeSet<CardId> = Default::default();

        if let Some(id) = self.parent_event {
            set.insert(id);
        }

        set
    }
}

#[async_trait::async_trait(?Send)]
pub trait CardTrait: Debug + Clone {
    async fn get_dependencies(&self) -> BTreeSet<CardId>;
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, Hash)]
pub enum CardType {
    Instance(InstanceCard),
    Normal(NormalCard),
    Unfinished(UnfinishedCard),
    Attribute(AttributeCard),
    Class(ClassCard),
    Statement(StatementCard),
    Event(EventCard),
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
            CardType::Instance(card) => card.get_dependencies().await,
            CardType::Normal(card) => card.get_dependencies().await,
            CardType::Unfinished(card) => card.get_dependencies().await,
            CardType::Attribute(card) => card.get_dependencies().await,
            CardType::Class(card) => card.get_dependencies().await,
            CardType::Statement(card) => card.get_dependencies().await,
            CardType::Event(card) => card.get_dependencies().await,
        }
    }

    pub async fn display_front(&self, provider: &CardProvider) -> String {
        match self {
            CardType::Instance(card) => card.name.clone(),
            CardType::Normal(card) => card.front.clone(),
            CardType::Unfinished(card) => card.front.clone(),
            CardType::Attribute(card) => card.display_front(provider).await,
            CardType::Class(card) => card.name.clone(),
            CardType::Statement(card) => card.front.clone(),
            CardType::Event(card) => card.front.clone(),
        }
    }
    pub fn backside(&self) -> Option<BackSide> {
        match self.clone() {
            CardType::Instance(InstanceCard { back, .. }) => back,
            CardType::Normal(NormalCard { back, .. }) => Some(back),
            CardType::Unfinished(_) => None,
            CardType::Attribute(AttributeCard { back, .. }) => Some(back),
            CardType::Class(ClassCard { back, .. }) => Some(back),
            CardType::Statement(_) => None,
            CardType::Event(_) => None,
        }
    }

    fn mut_backside(&mut self) -> Option<&mut BackSide> {
        match self {
            CardType::Instance(InstanceCard { back, .. }) => back.as_mut(),
            CardType::Normal(NormalCard { back, .. }) => Some(back),
            CardType::Unfinished(_) => None,
            CardType::Attribute(AttributeCard { back, .. }) => Some(back),
            CardType::Class(ClassCard { back, .. }) => Some(back),
            CardType::Statement(_) => None,
            CardType::Event(_) => None,
        }
    }

    // if a card is deleted that is being referenced we might have to change the card type
    pub fn remove_dep(&mut self, id: CardId) {
        if let Some(back) = self.mut_backside() {
            back.invalidate_if_has_ref(id);
        }

        match self {
            CardType::Instance(InstanceCard {
                ref name,
                ref back,
                class,
            }) => {
                if *class == id {
                    match back.clone() {
                        Some(backside) => {
                            *self = Self::Normal(NormalCard {
                                front: name.clone(),
                                back: backside,
                            })
                        }
                        None => {
                            *self = Self::Unfinished(UnfinishedCard {
                                front: name.clone(),
                            })
                        }
                    }
                }
            }
            CardType::Normal(_) => {}
            CardType::Unfinished(_) => {}
            CardType::Attribute(_) => {}
            CardType::Class(ClassCard {
                name,
                back,
                parent_class,
            }) => {
                if *parent_class == Some(id) {
                    *self = Self::Class(ClassCard {
                        name: name.clone(),
                        back: back.clone(),
                        parent_class: None,
                    });
                }
            }
            CardType::Statement(_) => {}
            CardType::Event(_) => {}
        };
    }

    pub fn type_name(&self) -> &str {
        match self {
            CardType::Unfinished(_) => "unfinished",
            CardType::Statement(_) => "statement",
            CardType::Attribute(_) => "attribute",
            CardType::Instance(_) => "instance",
            CardType::Normal(_) => "normal",
            CardType::Class(_) => "class",
            CardType::Event(_) => "event",
        }
    }

    /// This is mainly just so i dont forget to update the CType when the AnyType changes
    pub fn fieldless(&self) -> CType {
        match self {
            CardType::Instance(_) => CType::Instance,
            CardType::Normal(_) => CType::Normal,
            CardType::Unfinished(_) => CType::Unfinished,
            CardType::Attribute(_) => CType::Attribute,
            CardType::Class(_) => CType::Class,
            CardType::Statement(_) => CType::Statement,
            CardType::Event(_) => CType::Event,
        }
    }

    pub fn is_class(&self) -> bool {
        matches!(self, Self::Class(_))
    }
    pub fn is_instance(&self) -> bool {
        matches!(self, Self::Instance(_))
    }
    pub fn is_finished(&self) -> bool {
        !matches!(self, Self::Unfinished(_))
    }

    pub fn set_backside(self, new_back: BackSide) -> Self {
        match self {
            x @ CardType::Event(_) => x,
            x @ CardType::Instance(_) => x,
            x @ CardType::Statement(_) => x,
            CardType::Normal(NormalCard { front, .. }) => NormalCard {
                front,
                back: new_back,
            }
            .into(),
            CardType::Unfinished(UnfinishedCard { front }) => NormalCard {
                front,
                back: new_back,
            }
            .into(),
            CardType::Attribute(AttributeCard {
                attribute,
                instance: concept_card,
                ..
            }) => AttributeCard {
                attribute,
                back: new_back,
                instance: concept_card,
            }
            .into(),
            Self::Class(class) => ClassCard {
                name: class.name,
                back: new_back,
                parent_class: class.parent_class,
            }
            .into(),
        }
    }
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
struct RawType {
    ty: CType,
    front: Option<String>,
    back: Option<BackSide>,
    class: Option<Uuid>,
    instance: Option<Uuid>,
    attribute: Option<Uuid>,
    start_time: Option<String>,
    end_time: Option<String>,
    parent_event: Option<Uuid>,
}

impl RawType {
    pub fn class(&self) -> Option<Uuid> {
        self.class.clone()
    }
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct RawCard {
    id: Uuid,
    #[serde(flatten)]
    data: RawType,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    dependencies: BTreeSet<Uuid>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    tags: BTreeMap<String, String>,
    #[serde(default)]
    front_audio: Option<AudioId>,
    #[serde(default)]
    back_audio: Option<AudioId>,
}

pub fn bigrams(text: &str) -> Vec<[char;2]> {
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

impl RunLedger<CardEvent> for BaseCard {
    type Error = ();
    fn caches(&self) -> HashSet<String>{
        let mut out: HashSet<String> = Default::default();

        for dep in &self.dependencies {
            out.insert(CacheKey::Dependent(*dep).to_string());
        }

        for bigram in bigrams(&self.ty.raw_front()) {
            out.insert(CacheKey::Bigram(bigram).to_string());
        }

        match &self.ty {
            CardType::Instance(InstanceCard { class, .. }) => {
                out.insert(CacheKey::Instance(*class).to_string());
            },
            CardType::Normal(card) => {},
            CardType::Unfinished(card) => {},
            CardType::Attribute(AttributeCard {attribute, instance, ..}) => {
                out.insert(CacheKey::AttrClass(*instance).to_string());
                out.insert(CacheKey::AttrId(*attribute).to_string());

            },
            CardType::Class(ClassCard {parent_class, ..}) => {
                if let Some(class) = parent_class {
                    out.insert(CacheKey::SubClass(*class).to_string());
                }
            },
            CardType::Statement(card) => {},
            CardType::Event(card) => {},
        };

        

        if let Some(back) = self.ty.backside() {
            match back {
                BackSide::Text(_) => {},
                BackSide::Card(id) => {
                    out.insert(CacheKey::BackRef(id).to_string());
                },
                BackSide::List(ids) => {
                    for id in ids {
                        out.insert(CacheKey::BackRef(id).to_string());
                    }
                },
                BackSide::Time(_) => {},
                BackSide::Trivial => {},
                BackSide::Invalid => {},
            }
        }

        out.insert(CacheKey::CardType(self.ty.fieldless()).to_string());

        out
    }
    
    fn new_default(id: String) -> Self {
        Self {
            id: id.parse().unwrap(),
            ty: CardType::Normal(NormalCard { front: "uninit".to_string(), back: BackSide::Text("uninit".to_string()) }),
            dependencies: Default::default(),
            front_audio: Default::default(),
            back_audio: Default::default(),
        }
    }

    fn run_event(mut self, event: CardEvent) -> Result<Self, ()> {
        for action in event.action {
            match action {
                CardAction::SetFrontAudio ( audio ) => {
                    self.front_audio = audio;
                },
                CardAction::SetBackAudio ( audio ) => {
                    self.back_audio = audio;
                },
                CardAction::UpsertCard ( ty ) => {
                    self.ty = ty;
                },
                CardAction::DeleteCard => {},
                CardAction::AddDependency ( dependency ) => {
                    self.dependencies.insert(dependency);
                },
                CardAction::RemoveDependency ( dependency ) => {
                    self.dependencies.remove(&dependency);
                    self.ty.remove_dep(dependency);
                },
                CardAction::SetBackRef ( reff ) => {
                    let backside = BackSide::Card(reff);
                    self.ty = self.ty.set_backside(backside);
                },
            }
        }

        Ok(self)
    }

    fn derive_events(&self) -> Vec<CardEvent> {
        let mut actions = vec![];

        let action = CardAction::UpsertCard (self.ty.clone() );
        actions.push(action);

        if let Some(audio) = self.front_audio {
            let action = CardAction::SetFrontAudio ( Some(audio));
            actions.push(action);
        }

        if let Some(audio) = self.back_audio {
            let action = CardAction::SetFrontAudio ( Some(audio));
            actions.push(action);
        }


        for dep in &self.dependencies {
            let action = CardAction::AddDependency (*dep);
            actions.push(action);
        }

        let id = self.id;

        actions.into_iter().map(|action|CardEvent::new(id, action)).collect()
    }
    
    fn item_id(&self) -> String {
        self.id.to_string()
    }
}


#[derive(Ord, PartialOrd, Eq, Hash, PartialEq, Debug, Clone)]
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

fn into_any(raw: RawType) -> CardType {
    match raw.ty {
        CType::Instance => InstanceCard {
            name: raw.front.unwrap(),
            class: raw.class.unwrap(),
            back: raw.back,
        }
        .into(),
        CType::Normal => NormalCard {
            front: raw.front.unwrap(),
            back: raw.back.unwrap(),
        }
        .into(),
        CType::Unfinished => UnfinishedCard {
            front: raw.front.unwrap(),
        }
        .into(),
        CType::Attribute => AttributeCard {
            attribute: raw.attribute.unwrap(),
            back: raw.back.unwrap(),
            instance: raw.instance.unwrap(),
        }
        .into(),
        CType::Class => ClassCard {
            name: raw.front.unwrap(),
            back: raw.back.unwrap(),
            parent_class: raw.class,
        }
        .into(),
        CType::Statement => StatementCard {
            front: raw.front.unwrap(),
        }
        .into(),
        CType::Event => EventCard {
            front: raw.front.unwrap(),
            start_time: raw
                .start_time
                .clone()
                .map(TimeStamp::from_string)
                .flatten()
                .unwrap_or_default(),
            end_time: raw.end_time.clone().map(TimeStamp::from_string).flatten(),
            parent_event: raw.parent_event,
        }
        .into(),
    }
}

fn from_any(ty: CardType) -> RawType {
    let mut raw = RawType::default();
    let fieldless = ty.fieldless();
    raw.ty = fieldless;

    match ty {
        CardType::Instance(InstanceCard { name, class, back }) => {
            raw.class = Some(class);
            raw.front = Some(name);
            raw.back = back;
        }
        CardType::Normal(NormalCard { front, back }) => {
            raw.front = Some(front);
            raw.back = Some(back);
        }
        CardType::Unfinished(UnfinishedCard { front }) => {
            raw.front = Some(front);
        }
        CardType::Attribute(AttributeCard {
            attribute,
            back,
            instance,
        }) => {
            raw.attribute = Some(attribute);
            raw.back = Some(back);
            raw.instance = Some(instance);
        }
        CardType::Class(ClassCard {
            name,
            back,
            parent_class,
        }) => {
            raw.front = Some(name);
            raw.back = Some(back);
            raw.class = parent_class;
        }
        CardType::Statement(StatementCard { front }) => {
            raw.front = Some(front);
        }
        CardType::Event(EventCard {
            front,
            start_time,
            end_time,
            parent_event,
        }) => {
            raw.front = Some(front);
            raw.start_time = Some(start_time.serialize());
            raw.end_time = end_time.map(|t| t.serialize());
            raw.parent_event = parent_event;
        }
    };

    raw
}

