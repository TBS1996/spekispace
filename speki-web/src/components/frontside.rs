use std::fmt::Display;

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use speki_core::{audio::Audio, card::CType};
use strum::{EnumIter, IntoEnumIterator};
use tracing::info;

#[cfg(feature = "web")]
use crate::components::audioupload::AudioUpload;

use crate::components::{dropdown::DropComponent, DropDownMenu};

#[derive(EnumIter, Clone, Serialize, Deserialize, PartialEq)]
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

#[derive(PartialEq, Props, Clone)]
pub struct FrontPut {
    pub dropdown: DropDownMenu<CardTy>,
    pub text: Signal<String>,
    pub audio: Signal<Option<Audio>>,
}

#[cfg(feature = "desktop")]
#[component]
#[cfg(feature = "desktop")]
pub fn FrontPutRender(
    dropdown: DropDownMenu<CardTy>,
    mut text: Signal<String>,
    audio: Signal<Option<Audio>>,
) -> Element {
    use crate::components::set_card_link;

    let placeholder = "Front side";

    rsx! {
        div {
            class: "block text-gray-700 text-sm font-medium",
            "Front:"

            div {
                class: "backside-editor flex items-center space-x-4",

                div {
                    class: "flex-shrink-0",
                    style: "width: 65px;",
                    DropComponent {options: dropdown.options.clone(), selected: dropdown.selected.clone()}
                }

                input {
                    class: "bg-white w-full border border-gray-300 rounded-md p-2 mb-4 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                    placeholder: "{placeholder}",
                    value: "{text}",
                    oninput: move |evt| text.set(evt.value()),
                    onmouseup: move |e| {
                        let text = text.clone();
                        set_card_link(text, e.modifiers().shift());
                    },
                }


            }
        }
    }
}

#[cfg(feature = "web")]
#[component]
#[cfg(feature = "web")]
pub fn FrontPutRender(
    dropdown: DropDownMenu<CardTy>,
    text: Signal<String>,
    audio: Signal<Option<Audio>>,
) -> Element {
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
                    DropComponent {options: dropdown.options.clone(), selected: dropdown.selected.clone()}
                }


                AudioUpload { audio }

            }
        }
    }
}

impl FrontPut {
    pub fn new(default: CardTy) -> Self {
        Self {
            dropdown: DropDownMenu::new(CardTy::iter(), Some(default)),
            text: Signal::new_in_scope(Default::default(), ScopeId(3)),
            audio: Signal::new_in_scope(Default::default(), ScopeId(3)),
        }
    }

    pub fn reset(&self) {
        info!("resetting frontput");
        self.text.clone().set(Default::default());
    }
}
