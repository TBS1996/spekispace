use std::{fmt::Display, sync::Arc};

use serde::{Deserialize, Serialize};

use crate::Card;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum FloatFilter {
    Greater,
    Less,
    Any,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum IntFilter {
    Equal,
    Greater,
    Less,
    Any,
}

impl Display for IntFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            IntFilter::Greater => ">",
            IntFilter::Less => "<",
            IntFilter::Any => "any",
            IntFilter::Equal => "=",
        };

        write!(f, "{s}")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Hash)]
pub enum MyIntOrd {
    Equal,
    Greater,
    Less,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Hash)]
pub enum MyFloatOrd {
    Greater,
    Less,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FloatOp {
    pub num: f32,
    pub ord: MyFloatOrd,
}

impl From<MyFloatOrd> for FloatFilter {
    fn from(value: MyFloatOrd) -> Self {
        match value {
            MyFloatOrd::Greater => FloatFilter::Greater,
            MyFloatOrd::Less => FloatFilter::Less,
        }
    }
}

impl From<MyIntOrd> for IntFilter {
    fn from(value: MyIntOrd) -> Self {
        match value {
            MyIntOrd::Equal => IntFilter::Equal,
            MyIntOrd::Greater => IntFilter::Greater,
            MyIntOrd::Less => IntFilter::Less,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Hash)]
pub struct IntOp {
    pub num: u32,
    pub ord: MyIntOrd,
}

use std::hash::{Hash, Hasher};

impl Hash for FloatOp {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // f32 doesn't implement Hash by default because of NaN and other edge cases.
        // Here, we simply hash the underlying bit representation.
        self.num.to_bits().hash(state);
        self.ord.hash(state);
    }
}

impl Display for FloatFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            FloatFilter::Greater => ">",
            FloatFilter::Less => "<",
            FloatFilter::Any => "any",
        };

        write!(f, "{s}")
    }
}

/// Filter for cards.
/// Only uses the user-data part of cards, like reviews or custom tags.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default, Hash)]
pub struct CardFilter {
    pub recall: Option<FloatOp>,
    pub rec_recall: Option<FloatOp>,
    pub stability: Option<FloatOp>,
    pub rec_stability: Option<FloatOp>,
    pub suspended: Option<bool>,
    pub needs_work: Option<bool>,
    pub lapses: Option<IntOp>,
}

impl CardFilter {
    pub fn filter(&self, card: Arc<Card>) -> bool {
        let CardFilter {
            recall,
            rec_recall,
            stability,
            rec_stability,
            suspended,
            lapses,
            needs_work,
        } = self.clone();

        if let Some(flag) = suspended {
            if flag != card.is_suspended() {
                return false;
            }
        }

        if let Some(flag) = needs_work {
            if flag != card.needs_work() {
                return false;
            }
        }

        if let Some(IntOp { ord, num }) = lapses {
            let lapses = card.lapses() as u32;

            match ord {
                MyIntOrd::Equal => {
                    if lapses != num {
                        return false;
                    }
                }
                MyIntOrd::Greater => {
                    if lapses < num {
                        return false;
                    }
                }
                MyIntOrd::Less => {
                    if lapses > num {
                        return false;
                    }
                }
            }
        }

        if let Some(FloatOp { ord, num }) = recall {
            let recall = card.recall_rate().unwrap_or_default();

            match ord {
                MyFloatOrd::Greater => {
                    if recall < num {
                        return false;
                    }
                }
                MyFloatOrd::Less => {
                    if recall > num {
                        return false;
                    }
                }
            }
        }

        if let Some(FloatOp { ord, num }) = stability {
            let stability = card.maturity_days().unwrap_or_default();

            match ord {
                MyFloatOrd::Greater => {
                    if stability < num {
                        return false;
                    }
                }
                MyFloatOrd::Less => {
                    if stability > num {
                        return false;
                    }
                }
            }
        }

        if let Some(FloatOp { ord, num }) = rec_recall {
            let recall = card.min_rec_recall_rate();

            match ord {
                MyFloatOrd::Greater => {
                    if recall < num {
                        return false;
                    }
                }
                MyFloatOrd::Less => {
                    if recall > num {
                        return false;
                    }
                }
            }
        }
        if let Some(FloatOp { ord, num }) = rec_stability {
            let stability = card.min_rec_stability();

            match ord {
                MyFloatOrd::Greater => {
                    if stability < num {
                        return false;
                    }
                }
                MyFloatOrd::Less => {
                    if stability > num {
                        return false;
                    }
                }
            }
        }

        true
    }
}
