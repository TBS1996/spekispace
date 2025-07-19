use std::collections::BTreeSet;

use crate::{
    audio::AudioId,
    card::{AttributeId, Attrv2, BackSide, CardId, RawCard, TextData},
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
    SetBackBool(bool),
    SetBackTime(TimeStamp),
    SetDefaultQuestion(Option<String>),
    SetNamespace(Option<CardId>),
    InsertAttr(Attrv2),
    InsertAttrs(BTreeSet<Attrv2>),
    RemoveAttr(AttributeId),
    SetTrivial(bool),
    SetParentClass(Option<CardId>),
    SetInstanceClass(CardId),
    AttributeType {
        attribute: AttributeId,
        back: BackSide,
        instance: CardId,
    },
    NormalType {
        front: TextData,
        back: BackSide,
    },
    InstanceType {
        front: TextData,
        class: CardId,
    },
    StatementType {
        front: TextData,
    },
    ClassType {
        front: TextData,
    },
    UnfinishedType {
        front: TextData,
    },
    EventType {
        front: TextData,
        start_time: TimeStamp,
    },
    SetBackside(Option<BackSide>),
    SetFront(TextData),
}

pub enum HistoryEvent {
    Review { id: CardId, review: Review },
}

pub type MetaEvent = TheLedgerEvent<Metadata>;

#[derive(Debug, Clone, Serialize, Deserialize, Hash)]
pub enum MetaAction {
    Suspend(bool),
    SetTrivial(Option<bool>),
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
