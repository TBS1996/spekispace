use std::fmt::{Debug, Display};

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use speki_dto::BackSide;
use strum::{EnumIter, IntoEnumIterator};
use tracing::info;

use super::Komponent;
use crate::components::{CardRef, DropDownMenu};

#[derive(Clone)]
pub struct BackPut {
    pub text: Signal<String>,
    pub dropdown: DropDownMenu<BackOpts>,
    pub ref_card: CardRef,
}

impl Komponent for BackPut {
    fn render(&self) -> Element {
        rsx! {
            div {
                class: "block text-gray-700 text-sm font-medium mb-2 max-w-full",
                "Back:"

                div {
                    class: "backside-editor flex items-center space-x-4",

                    div {
                        class: "flex-grow overflow-hidden",
                        { match *self.dropdown.selected.read() {
                            BackOpts::Text => self.render_text(),
                            BackOpts::Card => self.ref_card.render(),
                        }}
                    }

                    div {
                        class: "flex-shrink-0",
                        style: "width: 65px;",
                        { self.dropdown.render() }
                    }
                }
            }
        }
    }
}

impl BackPut {
    pub fn new() -> Self {
        Self {
            text: Signal::new_in_scope(Default::default(), ScopeId(3)),
            dropdown: DropDownMenu::new(BackOpts::iter()),
            ref_card: CardRef::new(),
        }
    }

    pub fn reset(&self) {
        self.text.clone().set(Default::default());
        self.dropdown.reset();
        self.ref_card.reset();
    }

    pub fn to_backside(&self) -> Option<BackSide> {
        let chosen = self.dropdown.selected.cloned();
        info!("chosen is: {:?}", chosen);

        match chosen {
            BackOpts::Card => Some(BackSide::Card(self.ref_card.selected_card().cloned()?)),
            BackOpts::Text => {
                let s = self.text.cloned();
                info!("text is: {s}");

                if s.is_empty() {
                    return None;
                };

                Some(BackSide::Text(s))
            }
        }
    }

    fn render_text(&self) -> Element {
        let mut sig = self.text.clone();
        rsx! {
            input {
                class: "bg-white w-full border border-gray-300 rounded-md p-2 mb-4 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                value: "{sig}",
                oninput: move |evt| sig.set(evt.value()),
            }
        }
    }
}

#[derive(Default, Copy, Clone, Debug, Serialize, Deserialize, EnumIter)]
pub enum BackOpts {
    #[default]
    Text,
    Card,
}

impl Display for BackOpts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            BackOpts::Text => "🔤",
            BackOpts::Card => "🔗",
        };

        write!(f, "{s}")
    }
}
