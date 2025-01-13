use std::{cmp::Ordering, fmt::Display, sync::Arc};

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use speki_core::cardfilter::{CardFilter, FilterItem, MyNumOrd, NumOp, NumOrd};
use strum::{EnumIter, IntoEnumIterator};

use super::{DropDownMenu, Komponent};

#[derive(Clone, Debug)]
pub struct FilterEditor {
    filter_name: Signal<String>,
    rec_recall: FloatEntry,
    recall: FloatEntry,
    stability: FloatEntry,
    rec_stability: FloatEntry,
    lapses: FloatEntry,
    finished: BoolEntry,
    suspended: BoolEntry,
    pending: BoolEntry,
}

impl FilterEditor {
    pub fn new_default() -> Self {
        Self::new(default_filter())
    }
}

fn default_filter() -> CardFilter {
    CardFilter {
        recall: Some(NumOp {
            num: 0.8,
            ord: MyNumOrd::Less,
        }),
        rec_recall: Some(NumOp {
            num: 0.8,
            ord: MyNumOrd::Greater,
        }),
        stability: None,
        rec_stability: Some(NumOp {
            num: 10.,
            ord: MyNumOrd::Greater,
        }),
        finished: Some(true),
        suspended: Some(false),
        pending: None,
        lapses: None,
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, EnumIter)]
enum BoolOpt {
    True,
    False,
    Any,
}

impl Display for BoolOpt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            BoolOpt::True => "true",
            BoolOpt::False => "false",
            BoolOpt::Any => "any",
        };

        write!(f, "{s}")
    }
}

#[derive(Clone, Debug)]
struct BoolEntry {
    name: Arc<String>,
    opt: DropDownMenu<BoolOpt>,
}

impl BoolEntry {
    fn new(name: &str, opt: BoolOpt) -> Self {
        Self {
            name: Arc::new(name.to_string()),
            opt: DropDownMenu::new(BoolOpt::iter(), opt.into()),
        }
    }

    fn from_bool(name: &str, bool: Option<bool>) -> Self {
        let val: BoolOpt = match bool {
            Some(true) => BoolOpt::True,
            Some(false) => BoolOpt::False,
            None => BoolOpt::Any,
        };

        Self {
            name: Arc::new(name.to_string()),
            opt: DropDownMenu::new(vec![BoolOpt::Any, BoolOpt::True, BoolOpt::False], Some(val)),
        }
    }

    fn get_value(&self) -> Option<bool> {
        match self.opt.selected.cloned() {
            BoolOpt::True => Some(true),
            BoolOpt::False => Some(false),
            BoolOpt::Any => None,
        }
    }
}

#[derive(Clone, Debug)]
struct FloatEntry {
    input: Signal<String>,
    ord: DropDownMenu<NumOrd>,
    name: Arc<String>,
}

impl FloatEntry {
    fn new(name: &str, ord: NumOrd, val: Option<f32>) -> Self {
        Self {
            input: Signal::new_in_scope(
                val.map(|val| val.to_string()).unwrap_or_default(),
                ScopeId::APP,
            ),
            ord: DropDownMenu::new(
                vec![NumOrd::Any, NumOrd::Greater, NumOrd::Less, NumOrd::Equal],
                Some(ord),
            ),
            name: Arc::new(name.to_string()),
        }
    }

    fn from_numop(name: &str, op: Option<NumOp>) -> Self {
        let (ord, val) = match op {
            Some(NumOp { num, ord }) => (ord.into(), Some(num)),
            None => (NumOrd::Any, None),
        };

        Self::new(name, ord, val)
    }

    fn get_value(&self) -> Option<NumOp> {
        let input = self.input.cloned();

        if input.is_empty() {
            return None;
        }

        let ord: MyNumOrd = match self.ord.selected.cloned() {
            NumOrd::Equal => MyNumOrd::Equal,
            NumOrd::Greater => MyNumOrd::Greater,
            NumOrd::Less => MyNumOrd::Less,
            NumOrd::Any => return None,
        };

        let val: f32 = input.parse().unwrap();

        Some(NumOp { num: val, ord })
    }
}

impl FilterEditor {
    pub fn new(filter: CardFilter) -> Self {
        let filter_name = Signal::new_in_scope("new filter".to_string(), ScopeId::APP);
        let rec_recall = FloatEntry::from_numop("rec recall", filter.rec_recall);
        let recall = FloatEntry::from_numop("recall", filter.recall);
        let rec_stability = FloatEntry::from_numop("rec stability", filter.rec_stability);
        let stability = FloatEntry::from_numop("stability", filter.stability);
        let lapses = FloatEntry::from_numop("lapses", filter.lapses);

        let finished = BoolEntry::from_bool("finished", filter.finished);
        let suspended = BoolEntry::from_bool("suspended", filter.suspended);
        let pending = BoolEntry::from_bool("pending", filter.pending);

        Self {
            filter_name,
            rec_recall,
            recall,
            rec_stability,
            stability,
            lapses,
            finished,
            suspended,
            pending,
        }
    }

    pub fn to_filter(&self) -> CardFilter {
        CardFilter {
            recall: self.recall.get_value(),
            rec_recall: self.rec_recall.get_value(),
            stability: self.stability.get_value(),
            rec_stability: self.rec_stability.get_value(),
            finished: self.finished.get_value(),
            suspended: self.suspended.get_value(),
            pending: self.pending.get_value(),
            lapses: self.lapses.get_value(),
        }
    }
}

impl Komponent for FilterEditor {
    fn render(&self) -> Element {
        rsx! {
            div {
                class: "p-4 bg-white rounded-lg shadow-md flex flex-col gap-y-2 max-w-sm",

                {self.rec_recall.render()}
                {self.recall.render()}
                {self.stability.render()}
                {self.rec_stability.render()}
                {self.lapses.render()}
                {self.finished.render()}
                {self.suspended.render()}
                {self.pending.render()}
            }
        }
    }
}

impl Komponent for BoolEntry {
    fn render(&self) -> Element {
        rsx! {
            div {
                class: "flex items-center gap-x-2",
                label {
                    class: "text-sm font-medium text-gray-700 w-28",
                    "{self.name}:"
                }
                {self.opt.render()}
            }
        }
    }
}

impl Komponent for FloatEntry {
    fn render(&self) -> Element {
        let input = self.input.clone();
        rsx! {
            div {
                class: "flex items-center gap-x-2",
                label {
                    class: "text-sm font-medium text-gray-700 w-28",
                    "{self.name}:"
                }
                {self.ord.render()}
                if !matches!(self.ord.selected.cloned(), NumOrd::Any) {
                    input {
                        class: "w-20 p-1 border rounded focus:ring focus:ring-blue-200",
                        value: "{input}",
                        oninput: move |evt| {
                            let new_value = evt.value().clone();
                            if new_value.parse::<f64>().is_ok() || new_value.is_empty() {
                                input.clone().set(new_value);
                            }
                        },
                    }
                }
            }
        }
    }
}
