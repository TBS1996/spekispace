use std::time::Duration;

use crate::{card::CardId, recall_rate::Review, CardType};

pub struct Ledger(Vec<Event>);


enum Event {
    NewCard {
        id: CardId,
        ty: CardType,
    },
    Review{
        id: CardId,
        review: Review,
    },
    SetSuspend(bool),
}