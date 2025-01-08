use omtrent::TimeStamp;

use super::*;
use crate::{attribute::AttributeId, card_provider::CardProvider, App, Attribute};

#[async_trait::async_trait(?Send)]
impl CardTrait for NormalCard {
    async fn get_dependencies(&self) -> BTreeSet<CardId> {
        let mut set: BTreeSet<CardId> = Default::default();
        set.extend(self.back.dependencies().iter());
        set
    }

    async fn display_front(&self) -> String {
        self.front.clone()
    }
}

#[async_trait::async_trait(?Send)]
impl CardTrait for InstanceCard {
    async fn get_dependencies(&self) -> BTreeSet<CardId> {
        let mut set = BTreeSet::default();
        set.insert(self.class);
        set
    }

    async fn display_front(&self) -> String {
        self.name.clone()
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

    async fn display_front(&self) -> String {
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

#[async_trait::async_trait(?Send)]
impl CardTrait for UnfinishedCard {
    async fn get_dependencies(&self) -> BTreeSet<CardId> {
        Default::default()
    }

    async fn display_front(&self) -> String {
        self.front.clone()
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

    async fn display_front(&self) -> String {
        self.name.clone()
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

    async fn display_front(&self) -> String {
        self.front.clone()
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

        let CardType::Event(other) = other.ty else {
            panic!("wrong type");
        };

        self.inner_valid_sub_event(&other)
    }

    pub async fn valid_parent_event(&self, parent: CardId, app: &App) -> bool {
        let parent = app.load_card(parent).await.unwrap();
        let CardType::Event(parent) = parent.ty else {
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

    async fn display_front(&self) -> String {
        self.front.clone()
    }
}
