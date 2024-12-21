use std::fmt::Display;

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use strum::{EnumIter, IntoEnumIterator};

use super::Komponent;
use crate::components::DropDownMenu;

#[derive(EnumIter, Clone, Serialize, Deserialize)]
pub enum CardTy {
    Normal,
    Instance,
    Class,
    Unfinished,
}

impl Display for CardTy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            CardTy::Normal => "normal",
            CardTy::Instance => "instance",
            CardTy::Class => "class",
            CardTy::Unfinished => "unfinished",
        };

        write!(f, "{s}")
    }
}

#[derive(Clone)]
pub struct FrontPut {
    pub dropdown: DropDownMenu<CardTy>,
    pub text: Signal<String>,
}

impl Komponent for FrontPut {
    fn render(&self) -> Element {
        let mut text = self.text.clone();
        rsx! {
            div {
                class: "block text-gray-700 text-sm font-medium mb-2",
                "Front:"
                div {
                    class: "backside-editor flex items-center space-x-4",

                    input {
                        class: "bg-white w-full border border-gray-300 rounded-md p-2 mb-4 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                        value: "{text}",
                        oninput: move |evt| text.set(evt.value()),
                    }

                    { self.dropdown.render() }
                }

            }
        }
    }
}

impl FrontPut {
    pub fn new() -> Self {
        Self {
            dropdown: DropDownMenu::new(CardTy::iter()),
            text: Signal::new_in_scope(Default::default(), ScopeId(3)),
        }
    }

    pub fn reset(&self) {
        self.text.clone().set(Default::default());
    }
}
