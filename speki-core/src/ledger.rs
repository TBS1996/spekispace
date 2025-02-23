use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use speki_dto::LedgerEvent;
use crate::{audio::AudioId, card::{BaseCard, CardId}, recall_rate::Review, CardType};

pub fn decompose(card: &BaseCard) -> Vec<CardEvent> {
    if card.deleted {
        return vec![];
    }

    let mut actions = vec![];

    let action = CardAction::UpsertCard { ty: card.ty.clone() };
    actions.push(action);


    let action = CardAction::SetFrontAudio { audio: card.front_audio.clone() };
    actions.push(action);

    let action = CardAction::SetBackAudio { audio: card.back_audio.clone() };
    actions.push(action);

    for dep in &card.dependencies {
        let action = CardAction::AddDependency {dependency: *dep};
        actions.push(action);
    }

    let id = card.id;

    actions.into_iter().map(|action|CardEvent::new(id, action)).collect()
}

use speki_dto::RunLedger;

pub fn check_compose(mut old_card: BaseCard) {
    old_card.last_modified = Default::default();
    old_card.source = Default::default();
    let actions = decompose(&old_card);
    let mut card: Option<BaseCard> = None;

    for action in actions {
        let mut events: VecDeque<CardEvent> = VecDeque::from([action]);
        while let Some(event) = events.pop_front() {
            let (new_item, new_events) = BaseCard::run_event(card.clone(), event);
            card = Some(new_item);
            events.extend(new_events);
        }
    }
    let mut card = card.unwrap();
    card.last_modified = Default::default();
    card.source = Default::default();
    assert_eq!(&old_card, &card);
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CardEvent {
    pub action: CardAction,
    pub id: CardId,
}

impl CardEvent {
    pub fn new(id: CardId, action: CardAction) -> Self {
        Self {id, action}
    }
}

impl LedgerEvent<BaseCard> for CardEvent {
    fn id(&self) -> CardId {
        self.id
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub enum CardAction {
    SetFrontAudio{audio: Option<AudioId>},
    SetBackAudio{audio: Option<AudioId>},
    UpsertCard {
        ty: CardType,
    },
    DeleteCard,
    AddDependency {
        dependency: CardId,
    },
    RemoveDependency {
        dependency: CardId,
    },
    AddDependent {
        dependent: CardId,
    },
    RemoveDependent {
        dependent: CardId,
    },
    SetBackRef{
        reff: CardId,
    },
}


pub enum HistoryEvent {
    Review{
        id: CardId,
        review: Review,
    },
}

pub enum MetaEvent {
    SetSuspend{
        id: CardId,
        status: bool,
    },
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
impl From<HistoryEvent> for Event {
    fn from(event: HistoryEvent) -> Self {
        Event::History(event)
    }
}


pub enum Event {
    Meta(MetaEvent),
    History(HistoryEvent),
    Card(CardEvent),
}