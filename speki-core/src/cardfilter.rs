use std::{cmp::Ordering, fmt::Display, time::Duration};

use serde::{Deserialize, Serialize};
use speki_dto::{Item, ModifiedSource};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum NumOrd {
    Equal,
    Greater,
    Less,
    Any,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum MyNumOrd {
    Equal,
    Greater,
    Less,
}

impl From<MyNumOrd> for NumOrd {
    fn from(value: MyNumOrd) -> Self {
        match value {
            MyNumOrd::Equal => NumOrd::Equal,
            MyNumOrd::Greater => NumOrd::Greater,
            MyNumOrd::Less => NumOrd::Less,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NumOp {
    pub num: f32,
    pub ord: MyNumOrd,
}

impl Display for NumOrd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            NumOrd::Equal => "=",
            NumOrd::Greater => ">",
            NumOrd::Less => "<",
            NumOrd::Any => "any",
        };

        write!(f, "{s}")
    }
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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FilterItem {
    last_modified: Duration,
    name: String,
    deleted: bool,
    id: Uuid,
    source: ModifiedSource,
    filters: CardFilter,
}

impl Item for FilterItem {
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

    fn id(&self) -> uuid::Uuid {
        self.id
    }

    fn identifier() -> &'static str {
        "cardfilter"
    }

    fn source(&self) -> speki_dto::ModifiedSource {
        self.source
    }

    fn set_source(&mut self, source: speki_dto::ModifiedSource) {
        self.source = source;
    }
}
