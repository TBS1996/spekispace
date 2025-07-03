use std::{
    fmt::{Debug, Display},
    str::FromStr,
};

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use speki_core::card::BackSide;
use strum::{EnumIter, IntoEnumIterator};
use tracing::info;

use crate::{
    components::{
        cardref::CardRefRender, dropdown::DropComponent, set_card_link, CardRef, DropDownMenu,
    },
    overlays::card_selector::MyClosure,
};

/// Component to create the backside of a card
///
/// backside can be either text or a reference to another card
#[derive(PartialEq, Clone, Debug)]
pub struct BackPut {
    pub text: Signal<String>,
    pub dropdown: DropDownMenu<BackOpts>,
    pub ref_card: CardRef,
}

#[component]
pub fn TimestampRender(text: Signal<String>) -> Element {
    let mut sig = text.clone();
    let interpreted = omtrent::TimeStamp::from_str(&*sig.read())
        .map(|x| x.to_string())
        .unwrap_or_default();

    rsx! {
        div {
            class: "flex flex-row gap-2",
            input {
                class: "flex-1 bg-white border border-gray-300 rounded-md p-2 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                value: "{sig}",
                placeholder: "timestamp",
                oninput: move |evt| sig.set(evt.value()),
            }
            span {
                class: "flex-1 text-sm text-gray-500 p-2 bg-gray-50 border border-gray-300 rounded-md",
                "{interpreted}"
            }
        }
    }
}

#[component]
pub fn BackPutRender(
    text: Signal<String>,
    dropdown: DropDownMenu<BackOpts>,
    ref_card: CardRef,
) -> Element {
    rsx! {
        div {
        class: "block text-gray-700 text-sm font-medium max-w-full",


        div {
            class: "backside-editor flex items-center space-x-4 mb-4",

            div {
                class: "flex-shrink-0",
                style: "width: 80px;",
                DropComponent {
                    options: dropdown.options.clone(),
                    selected: dropdown.selected.clone(),
                }
            }

            div {
                class: "flex-grow overflow-hidden",

                match *dropdown.selected.read() {
                    BackOpts::Time => {
                        rsx!{TimestampRender { text }}
                    },
                    BackOpts::Text => {
                        let mut sig = text.clone();

                        rsx! {
                            input {
                                class: "bg-white w-full border border-gray-300 rounded-md p-2 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                                value: "{sig}",
                                placeholder: "back side",
                                oninput: move |evt| sig.set(evt.value()),
                                onmouseup: move |e| {
                                    let with_alias = e.modifiers().shift();
                                    let text = text.clone();
                                    set_card_link(text, with_alias);
                                },
                            }
                        }
                    },
                    BackOpts::Card => rsx! {
                        CardRefRender {
                            selected_card: ref_card.card.clone(),
                            placeholder: ref_card.placeholder.cloned(),
                            on_select: ref_card.on_select.clone(),
                            on_deselect: ref_card.on_deselect.clone(),
                            allowed: ref_card.allowed.clone(),
                            filter: ref_card.filter.clone(),
                        }
                    },
                }
            }
        }
    }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum BacksideError {
    InvalidTimestamp,
    MissingCard,
    MissingText,
}

impl BackPut {
    pub fn new(default: Option<BackSide>) -> Self {
        let default = default.unwrap_or_default();
        let ref_card = CardRef::new();
        if let Some(card) = default.as_card() {
            ref_card.set_ref(card);
        }

        let text = default.to_string();

        let backopt = if default.is_ref() {
            BackOpts::Card
        } else {
            BackOpts::Text
        };

        Self {
            text: Signal::new_in_scope(text, ScopeId(3)),
            dropdown: DropDownMenu::new(BackOpts::iter(), Some(backopt)),
            ref_card,
        }
    }

    pub fn on_deselect(mut self, f: MyClosure) -> Self {
        self.ref_card = self.ref_card.on_deselect(f);
        self
    }

    pub fn on_select(mut self, f: MyClosure) -> Self {
        self.ref_card = self.ref_card.on_select(f);
        self
    }

    pub fn reset(&self) {
        self.text.clone().set(Default::default());
        self.dropdown.reset();
        self.ref_card.reset();
    }

    pub fn try_to_backside(&self) -> Result<BackSide, BacksideError> {
        let chosen = self.dropdown.selected.cloned();
        info!("chosen is: {:?}", chosen);

        match chosen {
            BackOpts::Card => match self.ref_card.selected_card().cloned() {
                Some(card) => Ok(BackSide::Card(card)),
                None => Err(BacksideError::MissingCard),
            },
            BackOpts::Text => {
                let s = self.text.cloned();
                info!("text is: {s}");
                if s.is_empty() {
                    Err(BacksideError::MissingText)
                } else {
                    Ok(BackSide::Text(s.into()))
                }
            }
            BackOpts::Time => {
                let text = self.text.cloned();

                if text.is_empty() {
                    Err(BacksideError::InvalidTimestamp)
                } else {
                    match omtrent::TimeStamp::from_str(&*self.text.read()) {
                        Ok(ts) => Ok(BackSide::Time(ts)),
                        Err(_) => Err(BacksideError::InvalidTimestamp),
                    }
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
                Some(BackSide::Text(s.into()))
            }
            BackOpts::Time => match omtrent::TimeStamp::from_str(&*self.text.read()) {
                Ok(ts) => Some(BackSide::Time(ts)),
                Err(_) => None,
            },
        }
    }
}

#[derive(Default, Copy, Clone, Debug, Serialize, Deserialize, EnumIter, PartialEq)]
pub enum BackOpts {
    #[default]
    Text,
    Card,
    Time,
}

impl Display for BackOpts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            BackOpts::Text => "🔤",
            BackOpts::Card => "🔗",
            BackOpts::Time => "🕒",
        };

        write!(f, "{s}")
    }
}
