use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use speki_dto::LedgerEvent;
use crate::{audio::AudioId, card::{BaseCard, CardId}, collection::{CollectionId, DynCard, MaybeDyn}, metadata::Metadata, recall_rate::{Review, ReviewEvent}, CardType};

pub fn decompose(card: &BaseCard) -> Vec<CardEvent> {
    let mut actions = vec![];

    let action = CardAction::UpsertCard (card.ty.clone() );
    actions.push(action);


    let action = CardAction::SetFrontAudio ( card.front_audio.clone() );
    actions.push(action);

    let action = CardAction::SetBackAudio ( card.back_audio.clone() );
    actions.push(action);

    for dep in &card.dependencies {
        let action = CardAction::AddDependency (*dep);
        actions.push(action);
    }

    let id = card.id;

    actions.into_iter().map(|action|CardEvent::new(id, action)).collect()
}

use speki_dto::RunLedger;

pub fn check_compose(old_card: BaseCard) {
    let actions = decompose(&old_card);
    let mut card: BaseCard = BaseCard::new_default(old_card.id.to_string());

    for action in actions {
        card = card.run_event(action).unwrap();
    }

    assert_eq!(&old_card, &card);
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CardEvent {
    pub action: Vec<CardAction>,
    pub id: CardId,
}

impl CardEvent {
    pub fn new(id: CardId, action: CardAction) -> Self {
        Self {id, action: vec![action]}
    }
}

impl LedgerEvent for CardEvent {
    fn id(&self) -> String {
        self.id.to_string()
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub enum CardAction {
    SetFrontAudio(Option<AudioId>),
    SetBackAudio(Option<AudioId>),
    UpsertCard(CardType),
    DeleteCard,
    AddDependency(CardId),
    RemoveDependency(CardId),
    SetBackRef(CardId),
}


pub enum HistoryEvent {
    Review{
        id: CardId,
        review: Review,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaEvent {
    pub id: CardId,
    pub action: MetaAction,
}

impl LedgerEvent for MetaEvent {
    fn id(&self) -> String{
        self.id.to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CollectionEvent {
    pub action: CollectionAction,
    pub id: CollectionId,
}

impl CollectionEvent {
    pub fn new(id: CollectionId, action: CollectionAction) -> Self {
        Self {id, action}
    }
}

impl LedgerEvent for CollectionEvent {
    fn id(&self) -> String {
        self.id.to_string()
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub enum CollectionAction {
    SetName(String),
    InsertDyn(MaybeDyn),
    RemoveDyn(MaybeDyn),
}