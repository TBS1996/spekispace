use std::fmt::Display;

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use speki_dto::CType;
use strum::{EnumIter, IntoEnumIterator};
use tracing::info;

use super::Komponent;
use crate::{components::DropDownMenu, IS_SHORT};

#[derive(EnumIter, Clone, Serialize, Deserialize)]
pub enum CardTy {
    Normal,
    Instance,
    Class,
    Unfinished,
}

impl CardTy {
    pub fn to_ctype(&self) -> CType {
        match self {
            CardTy::Normal => CType::Normal,
            CardTy::Instance => CType::Instance,
            CardTy::Class => CType::Class,
            CardTy::Unfinished => CType::Unfinished,
        }
    }

    pub fn from_ctype(ty: CType) -> Self {
        match ty {
            CType::Instance => Self::Instance,
            CType::Normal => Self::Normal,
            CType::Unfinished => Self::Unfinished,
            CType::Attribute => Self::Normal,
            CType::Class => Self::Class,
            CType::Statement => Self::Unfinished,
            CType::Event => Self::Normal,
        }
    }
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
        let placeholder = if IS_SHORT.cloned() { "Front side" } else { "" };
        rsx! {
            div {
                class: "block text-gray-700 text-sm font-medium ",
                if !IS_SHORT() {
                    "Front:"
                }

                div {
                    class: "backside-editor flex items-center space-x-4",

                    input {
                        class: "bg-white w-full border border-gray-300 rounded-md p-2 mb-4 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                        placeholder: "{placeholder}",
                        value: "{text}",
                        oninput: move |evt| text.set(evt.value()),
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

impl FrontPut {
    pub fn new(default: CardTy) -> Self {
        Self {
            dropdown: DropDownMenu::new(CardTy::iter(), Some(default)),
            text: Signal::new_in_scope(Default::default(), ScopeId(3)),
        }
    }

    pub fn reset(&self) {
        info!("resetting frontput");
        self.text.clone().set(Default::default());
    }
}
