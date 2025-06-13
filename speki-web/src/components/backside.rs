use std::{
    fmt::{Debug, Display},
    str::FromStr,
};

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use speki_core::{audio::Audio, card::BackSide};
use strum::{EnumIter, IntoEnumIterator};
use tracing::info;

#[cfg(feature = "web")]
use crate::components::audioupload::AudioUpload;
use crate::{
    components::{cardref::CardRefRender, dropdown::DropComponent, CardRef, DropDownMenu},
    overlays::{card_selector::MyClosure, cardviewer::TempNode, OverlayEnum},
    APP, IS_SHORT,
};

/// Component to create the backside of a card
///
/// backside can be either text or a reference to another card
#[derive(PartialEq, Clone, Debug)]
pub struct BackPut {
    pub text: Signal<String>,
    pub dropdown: DropDownMenu<BackOpts>,
    pub ref_card: CardRef,
    pub audio: Signal<Option<Audio>>,
}
#[cfg(not(feature = "web"))]
#[component]
#[cfg(not(feature = "web"))]
pub fn BackPutRender(
    text: Signal<String>,
    dropdown: DropDownMenu<BackOpts>,
    ref_card: CardRef,
    overlay: Signal<Option<OverlayEnum>>,
    audio: Signal<Option<Audio>>,
) -> Element {
    use std::str::FromStr;

    use crate::components::set_card_link;

    rsx! {
        div {
            class: "block text-gray-700 text-sm font-medium max-w-full",

            if !IS_SHORT() {
                "Back:"
            }

            div {
                class: "backside-editor flex items-center space-x-4",


                div {
                    class: "flex-shrink-0",
                    style: "width: 65px;",
                    DropComponent {
                        options: dropdown.options.clone(),
                        selected: dropdown.selected.clone(),
                    }
                }

                div {
                    class: "flex-grow overflow-hidden",

                    {
                        match *dropdown.selected.read() {
                            BackOpts::Time => {
                                let placeholder = if IS_SHORT.cloned() {
                                    "Back side"
                                } else {
                                    ""
                                };
                                let mut sig = text.clone();
                                let interpreted = omtrent::TimeStamp::from_str(&*sig.read()).map(|x|x.to_string()).unwrap_or_default();


                                    rsx! {
                                        div {
                                            class: "flex flex-row gap-2",
                                            input {
                                                class: "flex-1 bg-white border border-gray-300 rounded-md p-2 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                                                value: "{sig}",
                                                placeholder: "{placeholder}",
                                                oninput: move |evt| sig.set(evt.value()),
                                            }

                                            span {
                                                class: "flex-1 text-sm text-gray-500 p-2 bg-gray-50 border border-gray-300 rounded-md",
                                                "{interpreted}"
                                            }
                                        }
                                    }

                            },
                            BackOpts::Text => {
                                let placeholder = if IS_SHORT.cloned() {
                                    "Back side"
                                } else {
                                    ""
                                };
                                let mut sig = text.clone();

                                rsx! {
                                    input {
                                        class: "bg-white w-full border border-gray-300 rounded-md p-2 mb-4 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                                        value: "{sig}",
                                        placeholder: "{placeholder}",
                                        oninput: move |evt| sig.set(evt.value()),
                                        onmouseup: move |e| {
                                            let with_alias = e.modifiers().shift();
                                            let text = text.clone();
                                            let overlay = overlay.clone();
                                            set_card_link(text, overlay, with_alias);
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
                                    dependent: ref_card.dependent.clone(),
                                    allowed: ref_card.allowed.clone(),
                                    filter: ref_card.filter.clone(),
                                    overlay,
                                }
                            },
                        }
                    }
                }

            }
        }
    }
}

#[cfg(feature = "web")]
#[component]
#[cfg(feature = "web")]
pub fn BackPutRender(
    text: Signal<String>,
    dropdown: DropDownMenu<BackOpts>,
    ref_card: CardRef,
    overlay: Signal<Option<OverlayEnum>>,
    audio: Signal<Option<Audio>>,
) -> Element {
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
                    { match *dropdown.selected.read() {
                        BackOpts::Text => {


        let placeholder = if IS_SHORT.cloned() { "Back side" } else { "" };
        let mut sig = text.clone();
        rsx! {
            input {
                class: "bg-white w-full border border-gray-300 rounded-md p-2 mb-4 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                value: "{sig}",
                placeholder: "{placeholder}",
                oninput: move |evt| sig.set(evt.value()),
            }
        }
                        },
                        BackOpts::Card => rsx!{ CardRefRender{
                            card_display: ref_card.display.clone(),
                            selected_card: ref_card.card.clone(),
                            placeholder: ref_card.placeholder.cloned(),
                            on_select: ref_card.on_select.clone(),
                            on_deselect: ref_card.on_deselect.clone(),
                            dependent: ref_card.dependent.clone(),
                            allowed: ref_card.allowed.clone(),
                            overlay,
                        }},
                    }}
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

impl BackPut {
    pub fn new(default: Option<BackSide>) -> Self {
        let default = default.unwrap_or_default();
        let ref_card = CardRef::new();
        if let Some(card) = default.as_card() {
            let card = APP.read().load_card_sync(card);
            ref_card.set_ref(card);
        }

        let backopt = if default.is_ref() {
            BackOpts::Card
        } else {
            BackOpts::Text
        };

        Self {
            text: Signal::new_in_scope(Default::default(), ScopeId(3)),
            audio: Signal::new_in_scope(Default::default(), ScopeId(3)),
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
