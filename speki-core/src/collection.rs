use std::time::Duration;

use serde::{Deserialize, Serialize};
use speki_dto::{Item, ModifiedSource};
use uuid::Uuid;

use crate::card::CardId;

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct Collection {
    pub id: Uuid,
    pub name: String,
    pub cards: Vec<CardId>,
    pub last_modified: Duration,
    pub deleted: bool,
    pub source: ModifiedSource,
}

impl Item for Collection {
    fn deleted(&self) -> bool {
        self.deleted
    }

    fn set_delete(&mut self) {
        self.deleted = true;
    }

    fn set_last_modified(&mut self, time: Duration) {
        self.last_modified = time;
    }

    fn last_modified(&self) -> Duration {
        self.last_modified
    }

    fn id(&self) -> Uuid {
        self.id
    }

    fn identifier() -> &'static str {
        "collections"
    }

    fn source(&self) -> ModifiedSource {
        self.source
    }

    fn set_source(&mut self, source: ModifiedSource) {
        self.source = source;
    }
}
