use std::{collections::BTreeSet, time::Duration};

use serde::{Deserialize, Serialize};
use speki_dto::{Item, ModifiedSource};

use crate::card::CardId;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Dependents {
    id: CardId,
    pub deps: BTreeSet<CardId>,
    source: ModifiedSource,
    deleted: bool,
    last_modified: Duration,
}

impl Dependents {
    pub fn new(card: CardId, deps: BTreeSet<CardId>, current_time: Duration) -> Self {
        Self {
            id: card,
            deps,
            source: Default::default(),
            deleted: false,
            last_modified: current_time,
        }

    }
}


impl Item for Dependents {
    type PreviousVersion = Dependents;

    fn deleted(&self) -> bool {
        false
    }

    fn set_delete(&mut self) {
        panic!("don't delete dependents cache!")
    }

    fn set_last_modified(&mut self, time: std::time::Duration) {
        self.last_modified = time;
    }

    fn last_modified(&self) -> std::time::Duration {
        self.last_modified
    }

    fn id(&self) -> uuid::Uuid {
        self.id
    }

    fn identifier() -> &'static str {
        "dependents"
    }

    fn source(&self) -> ModifiedSource {
        self.source
    }

    fn set_source(&mut self, source: ModifiedSource) {
        self.source = source;
    }
}