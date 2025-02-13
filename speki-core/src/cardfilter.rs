use std::{fmt::Display, sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use speki_dto::{Item, ModifiedSource};
use uuid::Uuid;

use crate::Card;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum NumOrd {
    Equal,
    Greater,
    Less,
    Any,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
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

/// Filter for cards.
/// Only uses the user-data part of cards, like reviews or custom tags.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
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

impl CardFilter {
    pub async fn filter(&self, card: Arc<Card>) -> bool {
        let CardFilter {
            recall,
            rec_recall,
            stability,
            rec_stability: _,
            finished,
            suspended,
            pending,
            lapses,
        } = self.clone();

        if let Some(NumOp { ord, num }) = recall {
            let recall = card.recall_rate().unwrap_or_default();

            match ord {
                MyNumOrd::Equal => {
                    if recall != num {
                        return false;
                    }
                }
                MyNumOrd::Greater => {
                    if recall < num {
                        return false;
                    }
                }
                MyNumOrd::Less => {
                    if recall > num {
                        return false;
                    }
                }
            }
        }

        if let Some(NumOp { ord, num }) = stability {
            let stability = card.maturity().unwrap_or_default();

            match ord {
                MyNumOrd::Equal => {
                    if stability != num {
                        return false;
                    }
                }
                MyNumOrd::Greater => {
                    if stability < num {
                        return false;
                    }
                }
                MyNumOrd::Less => {
                    if stability > num {
                        return false;
                    }
                }
            }
        }

        if let Some(NumOp { ord, num }) = rec_recall {
            let recall = card.min_rec_recall_rate().await;

            match ord {
                MyNumOrd::Equal => {
                    if recall != num {
                        return false;
                    }
                }
                MyNumOrd::Greater => {
                    if recall < num {
                        return false;
                    }
                }
                MyNumOrd::Less => {
                    if recall > num {
                        return false;
                    }
                }
            }
        }

        if let Some(NumOp { ord, num }) = lapses {
            let lapses = card.lapses() as f32;

            match ord {
                MyNumOrd::Equal => {
                    if lapses != num {
                        return false;
                    }
                }
                MyNumOrd::Greater => {
                    if lapses < num {
                        return false;
                    }
                }
                MyNumOrd::Less => {
                    if lapses > num {
                        return false;
                    }
                }
            }
        }

        if let Some(flag) = finished {
            if flag != card.is_finished() {
                return false;
            }
        }

        if let Some(flag) = suspended {
            if flag != card.is_suspended() {
                return false;
            }
        }

        if let Some(flag) = pending {
            if flag != card.is_pending() {
                return false;
            }
        }

        true
    }
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
    type PreviousVersion = Self;
    type Key = Uuid;

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
