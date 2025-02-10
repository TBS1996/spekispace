use std::time::Duration;

use serde::Deserialize;
use serde::Serialize;
use speki_dto::{Item, ModifiedSource};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Audio {
    pub id: Uuid,
    pub data: Vec<u8>,
    pub last_modified: Duration,
    pub deleted: bool,
    pub source: ModifiedSource,
}

impl Audio {
    pub fn new(data: Vec<u8>) -> Self {
        Self {
            id: AudioId::new_v4(),
            data,
            last_modified: Default::default(),
            deleted: false,
            source: Default::default(),
        }
    }
}

pub type AudioId = Uuid;

impl Item for Audio {
    type PreviousVersion = Self;

    fn deleted(&self) -> bool {
        self.deleted
    }

    fn set_delete(&mut self) {
        self.deleted = true;
    }

    fn set_last_modified(&mut self, time: std::time::Duration) {
        self.last_modified = time;
    }

    fn last_modified(&self) -> std::time::Duration {
        self.last_modified
    }

    fn id(&self) -> Uuid {
        self.id
    }

    fn identifier() -> &'static str {
        "audio"
    }

    fn source(&self) -> speki_dto::ModifiedSource {
        self.source
    }

    fn set_source(&mut self, source: speki_dto::ModifiedSource) {
        self.source = source;
    }
}
