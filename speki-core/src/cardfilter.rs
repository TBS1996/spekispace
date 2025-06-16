use std::{fmt::Display, sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Card;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum NumOrd {
    Equal,
    Greater,
    Less,
    Any,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Hash)]
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

use std::hash::{Hash, Hasher};

impl Hash for NumOp {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // f32 doesn't implement Hash by default because of NaN and other edge cases.
        // Here, we simply hash the underlying bit representation.
        self.num.to_bits().hash(state);
        self.ord.hash(state);
    }
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

/// Filter for cards.
/// Only uses the user-data part of cards, like reviews or custom tags.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default, Hash)]
pub struct CardFilter {
    pub recall: Option<NumOp>,
    pub rec_recall: Option<NumOp>,
    pub stability: Option<NumOp>,
    pub rec_stability: Option<NumOp>,
    pub finished: Option<bool>,
    pub suspended: Option<bool>,
    pub pending: Option<bool>,
    pub lapses: Option<NumOp>,
    pub isolated: Option<bool>,
}

impl CardFilter {
    pub fn filter(&self, card: Arc<Card>) -> bool {
        let CardFilter {
            recall,
            rec_recall,
            stability,
            rec_stability: _,
            finished,
            suspended,
            pending,
            lapses,
            isolated,
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
            let stability = card.maturity_days().unwrap_or_default();

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
            let recall = card.min_rec_recall_rate();

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

        if let Some(flag) = isolated {
            let no_edges = card.dependencies().is_empty() && card.dependents().is_empty();
            if flag != no_edges {
                return false;
            }
        }

        true
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Hash)]
pub struct FilterItem {
    last_modified: Duration,
    name: String,
    deleted: bool,
    id: Uuid,
    filters: CardFilter,
}
