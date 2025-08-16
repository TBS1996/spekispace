use std::{collections::BTreeMap, fmt::Display, sync::Arc, time::Duration};

use ledgerstore::{Ledger, Node};
use serde::{Deserialize, Serialize};

use crate::{
    card::{CardId, RawCard},
    metadata::Metadata,
    recall_rate::History,
    Card, CardProperty,
};

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

/// Card filter based on review history
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default, Hash)]
pub struct HistoryFilter {
    pub recall: Option<FloatOp>,
    pub rec_recall: Option<FloatOp>,
    pub stability: Option<FloatOp>,
    pub rec_stability: Option<FloatOp>,
    pub lapses: Option<IntOp>,
}

impl HistoryFilter {
    pub fn is_empty(&self) -> bool {
        let Self {
            recall,
            rec_recall,
            stability,
            rec_stability,
            lapses,
        } = self;

        recall.is_none()
            && rec_recall.is_none()
            && stability.is_none()
            && rec_stability.is_none()
            && lapses.is_none()
    }

    pub fn filter_old(&self, now: Duration, history: History, dependencies: Vec<History>) -> bool {
        let Self {
            recall,
            rec_recall,
            stability,
            rec_stability,
            lapses,
        } = self.clone();

        if let Some(IntOp { ord, num }) = lapses {
            let lapses = history.lapses() as u32;

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
            let recall = history.recall_rate(now).unwrap_or_default();

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
            let stability = history.maturity_days(now).unwrap_or_default();

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

        if !dependencies.is_empty() {
            if let Some(FloatOp { ord, num }) = rec_recall {
                let min_rec_recall: f32 = dependencies
                    .iter()
                    .map(|history| history.recall_rate(now).unwrap_or_default())
                    .fold(1.0, |acc, curr| if acc < curr { acc } else { curr });

                match ord {
                    MyFloatOrd::Greater => {
                        if min_rec_recall < num {
                            return false;
                        }
                    }
                    MyFloatOrd::Less => {
                        if min_rec_recall > num {
                            return false;
                        }
                    }
                }
            }
        }
        if !dependencies.is_empty() {
            if let Some(FloatOp { ord, num }) = rec_stability {
                let min_rec_stability: f32 = dependencies
                    .iter()
                    .map(|history| history.maturity_days(now).unwrap_or_default())
                    .fold(f32::MAX, |acc, curr| if acc < curr { acc } else { curr });

                match ord {
                    MyFloatOrd::Greater => {
                        if min_rec_stability < num {
                            return false;
                        }
                    }
                    MyFloatOrd::Less => {
                        if min_rec_stability > num {
                            return false;
                        }
                    }
                }
            }
        }

        true
    }

    pub fn filter(&self, state: RecallState) -> bool {
        let Self {
            recall,
            rec_recall,
            stability,
            rec_stability,
            lapses,
        } = self.clone();

        if let Some(IntOp { ord, num }) = lapses {
            let lapses = state.lapses;

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
            let recall = state.recall;

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
            let stability = state.stability_days;

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

        if let Some(min_rec_recall) = state.min_rec_recall {
            if let Some(FloatOp { ord, num }) = rec_recall {
                match ord {
                    MyFloatOrd::Greater => {
                        if min_rec_recall < num {
                            return false;
                        }
                    }
                    MyFloatOrd::Less => {
                        if min_rec_recall > num {
                            return false;
                        }
                    }
                }
            }
        }

        if let Some(min_rec_stab) = state.min_rec_stability_days {
            if let Some(FloatOp { ord, num }) = rec_stability {
                match ord {
                    MyFloatOrd::Greater => {
                        if min_rec_stab < num {
                            return false;
                        }
                    }
                    MyFloatOrd::Less => {
                        if min_rec_stab > num {
                            return false;
                        }
                    }
                }
            }
        }

        true
    }
}

/// Card filter based on metadata
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default, Hash)]
pub struct MetaFilter {
    pub suspended: Option<bool>,
    pub needs_work: Option<bool>,
}

impl MetaFilter {
    pub fn is_empty(&self) -> bool {
        self.suspended.is_none() && self.needs_work.is_none()
    }

    pub fn filter(&self, card: CardId, ledger: &Ledger<Metadata>) -> bool {
        let Self {
            suspended,
            needs_work,
        } = self.clone();

        let metadata = ledger.load_or_default(card);

        if let Some(flag) = suspended {
            if flag != metadata.suspended.is_suspended() {
                return false;
            }
        }

        if let Some(flag) = needs_work {
            if flag != metadata.needs_work {
                return false;
            }
        }

        true
    }
}

/// Filter for cards.
/// Only uses the user-data part of cards, like reviews or custom tags.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default, Hash)]
pub struct CardFilter {
    pub history: HistoryFilter,
    pub meta: MetaFilter,
}

impl CardFilter {
    pub fn filter(
        &self,
        card: CardId,
        recall: RecallState,
        meta_ledger: &Ledger<Metadata>,
    ) -> bool {
        let CardFilter { history, meta } = self.clone();

        if !meta.is_empty() {
            if !meta.filter(card, meta_ledger) {
                return false;
            }
        }

        history.filter(recall)
    }

    pub fn filter_old(&self, card: Arc<Card>, now: Duration, ledger: &Ledger<Metadata>) -> bool {
        let CardFilter { history, meta } = self.clone();

        if !meta.is_empty() {
            if !meta.filter(card.id(), ledger) {
                return false;
            }
        }

        let dependencies: Vec<History> = card
            .recursive_dependencies()
            .into_iter()
            .filter_map(|card| {
                if card.reviewable() {
                    Some(card.history().to_owned())
                } else {
                    None
                }
            })
            .collect();

        history.filter_old(now, card.history().to_owned(), dependencies)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RecallState {
    recall: f32,
    stability_days: f32,
    min_rec_recall: Option<f32>,
    min_rec_stability_days: Option<f32>,
    lapses: u32,
    pub pending: bool,
    pub reviewable: bool,
}

impl RecallState {
    pub fn eval_card(
        card: &Node<RawCard>,
        all: &mut BTreeMap<CardId, RecallState>,
        ledger: &Ledger<History>,
        card_ledger: &Ledger<RawCard>,
        time: Duration,
    ) -> Self {
        if let Some(state) = all.get(&card.id()) {
            return *state;
        }

        let history = ledger.load_or_default(card.id());
        let recall = history.recall_rate(time).unwrap_or_default();
        let stability = history.maturity_days(time).unwrap_or_default();

        let reviewable = card_ledger.has_property(
            card.id(),
            ledgerstore::PropertyCache {
                property: CardProperty::Reviewable,
                value: true.to_string(),
            },
        );

        let mut this = Self {
            recall,
            stability_days: stability,
            min_rec_recall: None,
            min_rec_stability_days: None,
            lapses: history.lapses(),
            pending: history.is_empty(),
            reviewable,
        };

        for dep in card.deps() {
            let recstate = Self::eval_card(dep, all, ledger, card_ledger, time);

            let min_recall = match (recstate.reviewable, recstate.min_rec_recall) {
                (true, Some(sub)) => Some(sub.min(recstate.recall)),
                (true, None) => Some(recstate.recall),
                (false, Some(sub)) => Some(sub),
                (false, None) => None,
            };

            let min_stab = match (recstate.reviewable, recstate.min_rec_stability_days) {
                (true, Some(sub)) => Some(sub.min(recstate.stability_days)),
                (true, None) => Some(recstate.stability_days),
                (false, Some(sub)) => Some(sub),
                (false, None) => None,
            };

            let this_rec = this.min_rec_recall.unwrap_or(1.0);

            if let Some(min_recall) = min_recall {
                if min_recall < this_rec {
                    this.min_rec_recall = Some(min_recall);
                }
            }

            let this_stab = this.min_rec_stability_days.unwrap_or(f32::MAX);

            if let Some(min_stab) = min_stab {
                if min_stab < this_stab {
                    this.min_rec_stability_days = Some(min_stab);
                }
            }
        }

        all.insert(card.id(), this);

        this
    }
}
