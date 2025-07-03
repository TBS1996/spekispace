use crate::{
    audio::AudioId,
    card::{AttributeId, Attrv2, CardId, RawCard},
    metadata::Metadata,
    recall_rate::{Review, ReviewEvent},
    CardType,
};
use ledgerstore::TheLedgerEvent;
use omtrent::TimeStamp;
use serde::{Deserialize, Serialize};

pub type CardEvent = TheLedgerEvent<RawCard>;

#[derive(Deserialize, Serialize, Clone, Debug, Hash)]
pub enum CardAction {
    SetFrontAudio(Option<AudioId>),
    SetBackAudio(Option<AudioId>),
    RemoveDependency(CardId),
    AddDependency(CardId),
    UpsertCard(CardType),
    SetBackRef(CardId),
    SetBackTime(TimeStamp),
    SetDefaultQuestion(Option<String>),
    SetNamespace(Option<CardId>),
    InsertAttr(Attrv2),
    RemoveAttr(AttributeId),
}

pub enum HistoryEvent {
    Review { id: CardId, review: Review },
}

pub type MetaEvent = TheLedgerEvent<Metadata>;

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

pub enum Event {
    Meta(MetaEvent),
    History(ReviewEvent),
    Card(CardEvent),
}
