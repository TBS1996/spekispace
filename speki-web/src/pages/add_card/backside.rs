use std::fmt::{Debug, Display};

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use speki_dto::BackSide;
use strum::{EnumIter, IntoEnumIterator};
use tracing::info;

use crate::components::{cardref::CardRef, dropdown::DropDownMenu};

#[derive(Clone)]
pub struct BackPut {
    text: Signal<String>,
    dropdown: DropDownMenu<BackOpts>,
    pub ref_card: CardRef,
}

impl BackPut {
    pub fn new() -> Self {
        Self {
            text: Default::default(),
            dropdown: DropDownMenu::new(BackOpts::iter()),
            ref_card: CardRef::new(Default::default()),
        }
    }

    pub fn reset(&self) {
        self.text.clone().set(Default::default());
        self.dropdown.reset();
        self.ref_card.reset();
    }

    pub fn render(&self) -> Element {
        rsx! {

            div {
                class: "block text-gray-700 text-sm font-medium mb-2",
                "Back:"

            div {
                class: "backside-editor flex items-center space-x-4",

                match *self.dropdown.selected.read() {
                    BackOpts::Text => self.render_text(),
                    BackOpts::Card => self.ref_card.render(),
                }

                { self.dropdown.view() }

            }
        }
        }
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
                class: "w-full border border-gray-300 rounded-md p-2 mb-4 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                value: "{sig}",
                oninput: move |evt| sig.set(evt.value()),
            }
        }
    }
}

#[derive(Default, Copy, Clone, Debug, Serialize, Deserialize, EnumIter)]
enum BackOpts {
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
