use std::{fmt::Display, sync::Arc};

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use speki_core::cardfilter::{
    CardFilter, FloatFilter, FloatOp, HistoryFilter, IntFilter, IntOp, MetaFilter, MyFloatOrd,
    MyIntOrd,
};
use strum::EnumIter;
use tracing::info;

use crate::components::dropdown::DropComponent;

use super::DropDownMenu;

/// Editor for creating a [`CardFilter`].
#[derive(Clone, Debug, PartialEq)]
pub struct FilterEditor {
    pub filter_name: Signal<String>,
    pub rec_recall: FloatEntry,
    pub recall: FloatEntry,
    pub stability: FloatEntry,
    pub rec_stability: FloatEntry,
    pub lapses: IntEntry,
    pub suspended: BoolEntry,
    pub needs_work: BoolEntry,
}

impl FilterEditor {
    pub fn new_default() -> Self {
        Self::new(default_filter())
    }

    pub fn new_permissive() -> Self {
        Self::new(CardFilter::default())
    }
}

pub fn default_filter() -> CardFilter {
    CardFilter {
        history: HistoryFilter {
            recall: Some(FloatOp {
                num: 0.9,
                ord: MyFloatOrd::Less,
            }),
            rec_recall: Some(FloatOp {
                num: 0.9,
                ord: MyFloatOrd::Greater,
            }),
            stability: None,
            rec_stability: Some(FloatOp {
                num: 10.,
                ord: MyFloatOrd::Greater,
            }),

            lapses: Some(IntOp {
                num: 4,
                ord: MyIntOrd::Less,
            }),
        },
        meta: MetaFilter {
            suspended: Some(false),
            needs_work: None,
        },
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, EnumIter, PartialEq)]
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

#[derive(Clone, Debug, PartialEq)]
pub struct BoolEntry {
    name: Arc<String>,
    opt: DropDownMenu<BoolOpt>,
}

impl BoolEntry {
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

    pub fn get_value(&self) -> Option<bool> {
        match self.opt.selected.cloned() {
            BoolOpt::True => Some(true),
            BoolOpt::False => Some(false),
            BoolOpt::Any => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct IntEntry {
    input: Signal<String>,
    ord: DropDownMenu<IntFilter>,
    name: Arc<String>,
}

impl IntEntry {
    fn new(name: &str, ord: IntFilter, val: Option<u32>) -> Self {
        Self {
            input: Signal::new_in_scope(
                val.map(|val| val.to_string()).unwrap_or_default(),
                ScopeId::APP,
            ),
            ord: DropDownMenu::new(
                vec![
                    IntFilter::Any,
                    IntFilter::Greater,
                    IntFilter::Less,
                    IntFilter::Equal,
                ],
                Some(ord),
            ),
            name: Arc::new(name.to_string()),
        }
    }

    fn from_numop(name: &str, op: Option<IntOp>) -> Self {
        let (ord, val): (IntFilter, Option<u32>) = match op {
            Some(IntOp { num, ord }) => (ord.into(), Some(num)),
            None => (IntFilter::Any, None),
        };

        Self::new(name, ord, val)
    }

    pub fn get_value(&self) -> Option<IntOp> {
        let input = self.input.cloned();

        if input.is_empty() {
            return None;
        }

        let ord: MyIntOrd = match self.ord.selected.cloned() {
            IntFilter::Greater => MyIntOrd::Greater,
            IntFilter::Less => MyIntOrd::Less,
            IntFilter::Equal => MyIntOrd::Equal,
            IntFilter::Any => return None,
        };

        let val: u32 = input.parse().unwrap();

        Some(IntOp { num: val, ord })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FloatEntry {
    input: Signal<String>,
    ord: DropDownMenu<FloatFilter>,
    name: Arc<String>,
}

impl FloatEntry {
    fn new(name: &str, ord: FloatFilter, val: Option<f32>) -> Self {
        Self {
            input: Signal::new_in_scope(
                val.map(|val| val.to_string()).unwrap_or_default(),
                ScopeId::APP,
            ),
            ord: DropDownMenu::new(
                vec![FloatFilter::Any, FloatFilter::Greater, FloatFilter::Less],
                Some(ord),
            ),
            name: Arc::new(name.to_string()),
        }
    }

    fn from_numop(name: &str, op: Option<FloatOp>) -> Self {
        let (ord, val): (FloatFilter, Option<f32>) = match op {
            Some(FloatOp { num, ord }) => (ord.into(), Some(num)),
            None => (FloatFilter::Any, None),
        };

        Self::new(name, ord, val)
    }

    pub fn get_value(&self) -> Option<FloatOp> {
        let input = self.input.cloned();

        if input.is_empty() {
            return None;
        }

        let ord: MyFloatOrd = match self.ord.selected.cloned() {
            FloatFilter::Greater => MyFloatOrd::Greater,
            FloatFilter::Less => MyFloatOrd::Less,
            FloatFilter::Any => return None,
        };

        let val: f32 = input.parse().unwrap();

        Some(FloatOp { num: val, ord })
    }
}

impl FilterEditor {
    pub fn new(filter: CardFilter) -> Self {
        let filter_name = Signal::new_in_scope("new filter".to_string(), ScopeId::APP);
        let rec_recall = FloatEntry::from_numop("rec recall", filter.history.rec_recall);
        let recall = FloatEntry::from_numop("recall", filter.history.recall);
        let rec_stability = FloatEntry::from_numop("rec stability", filter.history.rec_stability);
        let stability = FloatEntry::from_numop("stability", filter.history.stability);
        let lapses = IntEntry::from_numop("lapses", filter.history.lapses);

        let suspended = BoolEntry::from_bool("suspended", filter.meta.suspended);
        let needs_work = BoolEntry::from_bool("needs work", filter.meta.needs_work);

        Self {
            filter_name,
            rec_recall,
            recall,
            rec_stability,
            stability,
            lapses,
            suspended,
            needs_work,
        }
    }

    pub fn memo(&self) -> Memo<CardFilter> {
        let selv = self.clone();
        Signal::memo(move || {
            info!("cardfilter memo!");

            CardFilter {
                history: HistoryFilter {
                    recall: selv.recall.get_value(),
                    rec_recall: selv.rec_recall.get_value(),
                    stability: selv.stability.get_value(),
                    rec_stability: selv.rec_stability.get_value(),
                    lapses: selv.lapses.get_value(),
                },
                meta: MetaFilter {
                    suspended: selv.suspended.get_value(),
                    needs_work: selv.needs_work.get_value(),
                },
            }
        })
    }

    pub fn to_filter(&self) -> CardFilter {
        CardFilter {
            history: HistoryFilter {
                recall: self.recall.get_value(),
                rec_recall: self.rec_recall.get_value(),
                stability: self.stability.get_value(),
                rec_stability: self.rec_stability.get_value(),
                lapses: self.lapses.get_value(),
            },
            meta: MetaFilter {
                suspended: self.suspended.get_value(),
                needs_work: self.needs_work.get_value(),
            },
        }
    }
}

#[component]
pub fn FilterComp(editor: FilterEditor) -> Element {
    let FilterEditor {
        filter_name: _,
        rec_recall,
        recall,
        stability,
        rec_stability,
        lapses,
        suspended,
        needs_work,
    } = editor;
    rsx! {
        div {
            class: "mr-4 ml-2 bg-white rounded-md shadow-sm flex flex-col gap-y-2",

            FloatEntryRender { input: rec_recall.input.clone(), ord: rec_recall.ord.clone(), name: rec_recall.name.clone() },
            FloatEntryRender { input: recall.input.clone(), ord: recall.ord.clone(), name: recall.name.clone() },
            FloatEntryRender { input: stability.input.clone(), ord: stability.ord.clone(), name: stability.name.clone() },
            FloatEntryRender { input: rec_stability.input.clone(), ord: rec_stability.ord.clone(), name: rec_stability.name.clone() },
            IntEntryRender { input: lapses.input.clone(), ord: lapses.ord.clone(), name: lapses.name.clone() },
            BoolEntryRender { name: suspended.name.clone(), opt: suspended.opt.clone() },
            BoolEntryRender { name: needs_work.name.clone(), opt: needs_work.opt.clone() },
        }
    }
}

#[component]
fn BoolEntryRender(name: Arc<String>, opt: DropDownMenu<BoolOpt>) -> Element {
    rsx! {
        div {
            class: "flex items-center gap-x-2",
            label {
                class: "text-sm font-medium text-gray-700 w-28",
                "{name}:"
            }

            DropComponent{options: opt.options.clone(), selected: opt.selected.clone()}
        }
    }
}

#[component]
fn IntEntryRender(
    input: Signal<String>,
    ord: DropDownMenu<IntFilter>,
    name: Arc<String>,
) -> Element {
    let input = input.clone();
    rsx! {
        div {
            class: "flex items-center gap-x-2",
            label {
                class: "text-sm font-medium text-gray-700 w-28",
                "{name}:"
            }

            DropComponent {options: ord.options.clone(), selected: ord.selected.clone()  }

            if !matches!(ord.selected.cloned(), IntFilter::Any) {
                input {
                    class: "w-20 p-1 border rounded focus:ring focus:ring-blue-200",
                    value: "{input}",
                    oninput: move |evt| {
                        let new_value = evt.value().clone();
                        if new_value.parse::<u32>().is_ok() || new_value.is_empty() {
                            input.clone().set(new_value);
                        }
                    },
                }
            }
        }
    }
}

#[component]
fn FloatEntryRender(
    input: Signal<String>,
    ord: DropDownMenu<FloatFilter>,
    name: Arc<String>,
) -> Element {
    let input = input.clone();
    rsx! {
        div {
            class: "flex items-center gap-x-2",
            label {
                class: "text-sm font-medium text-gray-700 w-28",
                "{name}:"
            }

            DropComponent {options: ord.options.clone(), selected: ord.selected.clone()  }

            if !matches!(ord.selected.cloned(), FloatFilter::Any) {
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
