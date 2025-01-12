use std::time::Duration;

use serde::{Deserialize, Serialize};
use speki_dto::{Item, ModifiedSource};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum NumOrd {
    Equal,
    Greater,
    Less,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NumOp {
    num: f32,
    ord: NumOrd,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ItemData {
    last_modified: Duration,
    deleted: bool,
    id: Uuid,
    source: ModifiedSource,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CardFilter {
    pub recall: Option<NumOp>,
    pub rec_recall: Option<NumOp>,
    pub stability: Option<NumOp>,
    pub rec_stability: Option<NumOp>,
    pub finished: Option<bool>,
    pub suspended: Option<bool>,
    pub pending: Option<bool>,
    pub lapses: Option<NumOp>,
    meta: ItemData,
}

impl Item for CardFilter {
    fn deleted(&self) -> bool {
        self.meta.deleted
    }

    fn set_delete(&mut self) {
        self.meta.deleted = true;
    }

    fn set_last_modified(&mut self, time: std::time::Duration) {
        self.meta.last_modified = time;
    }

    fn last_modified(&self) -> std::time::Duration {
        self.meta.last_modified
    }

    fn id(&self) -> uuid::Uuid {
        self.meta.id
    }

    fn identifier() -> &'static str {
        "cardfilter"
    }

    fn source(&self) -> speki_dto::ModifiedSource {
        self.meta.source
    }

    fn set_source(&mut self, source: speki_dto::ModifiedSource) {
        self.meta.source = source;
    }
}
