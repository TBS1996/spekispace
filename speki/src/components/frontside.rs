use std::fmt::Display;

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use speki_core::card::{CType, CardId};
use strum::{EnumIter, IntoEnumIterator};
use tracing::info;

use crate::components::{dropdown::DropComponent, DropDownMenu};

#[derive(EnumIter, Clone, Serialize, Deserialize, PartialEq, Debug)]
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

#[derive(PartialEq, Props, Clone, Debug)]
pub struct FrontPut {
    pub dropdown: DropDownMenu<CardTy>,
    pub text: Signal<String>,
}

impl FrontPut {
    pub fn is_empty(&self) -> bool {
        self.text.read().is_empty()
    }
}

#[component]
pub fn FrontPutRender(
    dropdown: DropDownMenu<CardTy>,
    text: Signal<String>,
    #[props(default = false)] disabled: bool,
    #[props(default)] card_id: Option<CardId>,
) -> Element {
    use crate::components::LinkEditor;

    rsx! {
        div {
            class: "block text-gray-700 text-sm font-medium",
            div {
                class: "backside-editor flex items-center space-x-4 mb-4",

                div {
                    class: "flex-shrink-0",
                    style: "width: 80px;",
                    DropComponent {options: dropdown.options.clone(), selected: dropdown.selected.clone(), disabled}
                }

                div {
                    class: "flex-grow overflow-hidden",
                    LinkEditor {
                        key: "{card_id:?}",
                        text,
                        placeholder: "front side".to_string(),
                        disabled,
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
