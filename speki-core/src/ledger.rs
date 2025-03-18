
use crate::{
    audio::AudioId,
    card::CardId,
    collection::{CollectionId, MaybeDyn},
    recall_rate::{History, Review, ReviewEvent},
    CardType,
};
use serde::{Deserialize, Serialize};
use speki_dto::LedgerEvent;

pub fn decompose_history(history: History) -> Vec<ReviewEvent> {
    let mut actions = vec![];

    let id = history.id;
    for review in history.reviews {
        actions.push(ReviewEvent {
            id,
            grade: review.grade,
            timestamp: review.timestamp,
        });
    }

    actions
}


#[derive(Serialize, Deserialize, Clone, Debug, Hash)]
pub struct CardEvent {
    pub action: Vec<CardAction>,
    pub id: CardId,
}

impl CardEvent {
    pub fn new(id: CardId, action: CardAction) -> Self {
        Self {
            id,
            action: vec![action],
        }
    }
}

impl LedgerEvent for CardEvent {
    type Key = CardId;

    fn id(&self) -> Self::Key {
        self.id
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, Hash)]
pub enum CardAction {
    SetFrontAudio(Option<AudioId>),
    SetBackAudio(Option<AudioId>),
    RemoveDependency(CardId),
    AddDependency(CardId),
    UpsertCard(CardType),
    SetBackRef(CardId),
    DeleteCard,
}

pub enum HistoryEvent {
    Review { id: CardId, review: Review },
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash)]
pub struct MetaEvent {
    pub id: CardId,
    pub action: MetaAction,
}

impl LedgerEvent for MetaEvent {
    type Key = CardId;

    fn id(&self) -> Self::Key {
        self.id
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash)]
pub enum MetaAction {
    Suspend(bool),
}

impl From<MetaEvent> for Event {
    fn from(event: MetaEvent) -> Self {
        Event::Meta(event)
    }
}
impl From<CardEvent> for Event {
    fn from(event: CardEvent) -> Self {
        Event::Card(event)
    }
}
impl From<ReviewEvent> for Event {
    fn from(event: ReviewEvent) -> Self {
        Event::History(event)
    }
}
impl From<CollectionEvent> for Event {
    fn from(event: CollectionEvent) -> Self {
        Event::Collection(event)
    }
}

pub enum Event {
    Meta(MetaEvent),
    History(ReviewEvent),
    Card(CardEvent),
    Collection(CollectionEvent),
}

#[derive(Serialize, Deserialize, Clone, Debug, Hash)]
pub struct CollectionEvent {
    pub action: CollectionAction,
    pub id: CollectionId,
}

impl CollectionEvent {
    pub fn new(id: CollectionId, action: CollectionAction) -> Self {
        Self { id, action }
    }
}

impl LedgerEvent for CollectionEvent {
    type Key = CollectionId;

    fn id(&self) -> Self::Key {
        self.id
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, Hash)]
pub enum CollectionAction {
    SetName(String),
    InsertDyn(MaybeDyn),
    RemoveDyn(MaybeDyn),
}
