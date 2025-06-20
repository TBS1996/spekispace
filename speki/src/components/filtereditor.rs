use std::{fmt::Display, sync::Arc};

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use speki_core::cardfilter::{CardFilter, MyNumOrd, NumOp, NumOrd};
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
    pub lapses: FloatEntry,
    pub suspended: BoolEntry,
}

impl FilterEditor {
    pub fn new_default() -> Self {
        Self::new(default_filter())
    }

    pub fn new_permissive() -> Self {
        Self::new(CardFilter::default())
    }
}

fn default_filter() -> CardFilter {
    CardFilter {
        recall: Some(NumOp {
            num: 0.9,
            ord: MyNumOrd::Less,
        }),
        rec_recall: Some(NumOp {
            num: 0.9,
            ord: MyNumOrd::Greater,
        }),
        stability: None,
        rec_stability: Some(NumOp {
            num: 10.,
            ord: MyNumOrd::Greater,
        }),
        suspended: Some(false),
        lapses: Some(NumOp {
            num: 4.,
            ord: MyNumOrd::Less,
        }),
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
pub struct FloatEntry {
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

    pub fn get_value(&self) -> Option<NumOp> {
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

        let suspended = BoolEntry::from_bool("suspended", filter.suspended);

        Self {
            filter_name,
            rec_recall,
            recall,
            rec_stability,
            stability,
            lapses,
            suspended,
        }
    }

    pub fn memo(&self) -> Memo<CardFilter> {
        let selv = self.clone();
        Signal::memo(move || {
            info!("cardfilter memo!");

            CardFilter {
                recall: selv.recall.get_value(),
                rec_recall: selv.rec_recall.get_value(),
                stability: selv.stability.get_value(),
                rec_stability: selv.rec_stability.get_value(),
                suspended: selv.suspended.get_value(),
                lapses: selv.lapses.get_value(),
            }
        })
    }

    pub fn to_filter(&self) -> CardFilter {
        CardFilter {
            recall: self.recall.get_value(),
            rec_recall: self.rec_recall.get_value(),
            stability: self.stability.get_value(),
            rec_stability: self.rec_stability.get_value(),
            suspended: self.suspended.get_value(),
            lapses: self.lapses.get_value(),
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
    } = editor;
    rsx! {
        div {
            class: "mr-4 p-4 bg-white rounded-lg shadow-md flex flex-col gap-y-2 max-w-sm",


            FloatEntryRender { input: rec_recall.input.clone(), ord: rec_recall.ord.clone(), name: rec_recall.name.clone() },
            FloatEntryRender { input: recall.input.clone(), ord: recall.ord.clone(), name: recall.name.clone() },
            FloatEntryRender { input: stability.input.clone(), ord: stability.ord.clone(), name: stability.name.clone() },
            FloatEntryRender { input: rec_stability.input.clone(), ord: rec_stability.ord.clone(), name: rec_stability.name.clone() },
            FloatEntryRender { input: lapses.input.clone(), ord: lapses.ord.clone(), name: lapses.name.clone() },
            BoolEntryRender { name: suspended.name.clone(), opt: suspended.opt.clone() },
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
fn FloatEntryRender(
    input: Signal<String>,
    ord: DropDownMenu<NumOrd>,
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

            if !matches!(ord.selected.cloned(), NumOrd::Any) {
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
