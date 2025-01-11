use omtrent::TimeStamp;

use super::*;
use crate::{attribute::AttributeId, card_provider::CardProvider, App, Attribute};

/// Represents the card without userdata, the part that can be freely shared among different users.
#[derive(Clone)]
pub struct BaseCard {
    pub id: CardId,
    pub ty: CardType,
    pub dependencies: BTreeSet<CardId>,
    pub last_modified: Duration,
    pub source: ModifiedSource,
}

impl BaseCard {
    pub fn from_raw(raw: RawCard, provider: CardProvider) -> Self {
        Self {
            id: raw.id,
            ty: into_any(raw.data, &provider),
            dependencies: raw.dependencies,
            last_modified: raw.last_modified,
            source: raw.source,
        }
    }
}

impl From<BaseCard> for RawCard {
    fn from(card: BaseCard) -> Self {
        RawCard {
            id: card.id,
            data: from_any(card.ty),
            dependencies: card.dependencies,
            deleted: false,
            last_modified: card.last_modified,
            source: card.source,
            tags: Default::default(),
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
#[derive(Debug, Clone)]
pub struct UnfinishedCard {
    pub front: String,
}

/// Just a normal flashcard
#[derive(Debug, Clone)]
pub struct NormalCard {
    pub front: String,
    pub back: BackSide,
}

/// A class, which is something that has specific instances of it, but is not a single thing in itself.
/// A class might also have sub-classes, for example, the class chemical element has a sub-class isotope
#[derive(Debug, Clone)]
pub struct ClassCard {
    pub name: String,
    pub back: BackSide,
    pub parent_class: Option<CardId>,
}

/// An attribute describes a specific instance of a class. For example the class Person can have attribute "when was {} born?"
/// this will be applied to all instances of the class and its subclasses
#[derive(Debug, Clone)]
pub struct AttributeCard {
    pub attribute: AttributeId,
    pub back: BackSide,
    pub instance: CardId,
    pub card_provider: CardProvider,
}

impl AttributeCard {
    pub async fn display_front(&self) -> String {
        self.card_provider
            .provider
            .attrs
            .load_item(self.attribute)
            .await
            .map(|dto| Attribute::from_dto(dto, self.card_provider.clone()))
            .unwrap()
            .name(self.instance)
            .await
            .unwrap_or_else(|| "oops, instance is deleted".to_string())
    }
}

/// A specific instance of a class
/// For example, the instance might be Elvis Presley where the concept would be "Person"
/// the right answer is to know which class the instance belongs to
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
pub struct StatementCard {
    pub front: String,
}

#[async_trait::async_trait(?Send)]
impl CardTrait for StatementCard {
    async fn get_dependencies(&self) -> BTreeSet<CardId> {
        Default::default()
    }
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
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

    pub async fn display_front(&self) -> String {
        match self {
            CardType::Instance(card) => card.name.clone(),
            CardType::Normal(card) => card.front.clone(),
            CardType::Unfinished(card) => card.front.clone(),
            CardType::Attribute(card) => card.display_front().await,
            CardType::Class(card) => card.name.clone(),
            CardType::Statement(card) => card.front.clone(),
            CardType::Event(card) => card.front.clone(),
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

    pub fn set_backside(self, new_back: BackSide, card_provider: &CardProvider) -> Self {
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
                card_provider: card_provider.clone(),
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
pub struct RawType {
    pub ty: CType,
    pub front: Option<String>,
    pub back: Option<BackSide>,
    pub class: Option<Uuid>,
    pub instance: Option<Uuid>,
    pub attribute: Option<Uuid>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub parent_event: Option<Uuid>,
}

impl RawType {
    pub fn class(&self) -> Option<Uuid> {
        self.class.clone()
    }
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct RawCard {
    pub id: Uuid,
    #[serde(flatten)]
    pub data: RawType,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub dependencies: BTreeSet<Uuid>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tags: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub deleted: bool,
    #[serde(default)]
    pub last_modified: Duration,
    #[serde(default)]
    pub source: ModifiedSource,
}

impl Item for RawCard {
    fn last_modified(&self) -> Duration {
        self.last_modified
    }

    fn set_last_modified(&mut self, time: Duration) {
        self.last_modified = time;
    }

    fn set_source(&mut self, source: ModifiedSource) {
        self.source = source;
    }

    fn source(&self) -> ModifiedSource {
        self.source
    }

    fn id(&self) -> Uuid {
        self.id
    }

    fn identifier() -> &'static str {
        "cards"
    }

    fn deleted(&self) -> bool {
        self.deleted
    }

    fn set_delete(&mut self) {
        self.deleted = true;
    }
}

pub type CardId = Uuid;

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

fn is_false(flag: &bool) -> bool {
    !flag
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Config;

#[derive(Serialize, Deserialize, Debug, Clone, Default, Copy, Eq, PartialEq)]
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
