use std::{
    fmt::{Debug, Display},
    sync::Arc,
};

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use speki_core::{AnyType, Card};
use speki_dto::BackSide;
use strum::{EnumIter, IntoEnumIterator};
use tracing::info;

use super::Komponent;
use crate::{
    components::{CardRef, DropDownMenu},
    overlays::cardviewer::TempNode,
    APP, IS_SHORT,
};

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
                class: "block text-gray-700 text-sm font-medium max-w-full",
                if !IS_SHORT() {
                    "Back:"
                }

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
    pub fn new(default: Option<BackSide>) -> Self {
        let default = default.unwrap_or_default();
        let ref_card = CardRef::new();
        if let Some(card) = default.as_card() {
            let refc = ref_card.clone();
            spawn(async move {
                let card = APP.read().load_card(card).await;
                refc.set_ref(card).await;
            });
        }

        let backopt = if default.is_ref() {
            BackOpts::Card
        } else {
            BackOpts::Text
        };

        Self {
            text: Signal::new_in_scope(Default::default(), ScopeId(3)),
            dropdown: DropDownMenu::new(BackOpts::iter(), Some(backopt)),
            ref_card,
        }
    }

    pub fn with_deselect(mut self, f: Arc<Box<dyn Fn(Arc<Card<AnyType>>)>>) -> Self {
        self.ref_card = self.ref_card.with_deselect(f);
        self
    }

    pub fn with_closure(mut self, f: Arc<Box<dyn Fn(Arc<Card<AnyType>>)>>) -> Self {
        self.ref_card = self.ref_card.with_closure(f);
        self
    }

    pub fn reset(&self) {
        self.text.clone().set(Default::default());
        self.dropdown.reset();
        self.ref_card.reset();
    }

    pub fn with_dependents(mut self, deps: TempNode) -> Self {
        self.ref_card = self.ref_card.with_dependents(deps);
        self
    }

    pub fn to_backside(&self) -> Option<BackSide> {
        let chosen = self.dropdown.selected.cloned();
        info!("chosen is: {:?}", chosen);

        match chosen {
            BackOpts::Card => Some(BackSide::Card(self.ref_card.selected_card().cloned()?)),
            BackOpts::Text => {
                let s = self.text.cloned();
                info!("text is: {s}");
                Some(BackSide::Text(s))
            }
        }
    }

    fn render_text(&self) -> Element {
        let placeholder = if IS_SHORT.cloned() { "Back side" } else { "" };
        let mut sig = self.text.clone();
        rsx! {
            input {
                class: "bg-white w-full border border-gray-300 rounded-md p-2 mb-4 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                value: "{sig}",
                placeholder: "{placeholder}",
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
