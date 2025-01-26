use std::fmt::{Debug, Display};

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use speki_core::{audio::Audio, card::BackSide};
use strum::{EnumIter, IntoEnumIterator};
use tracing::info;

use crate::{
    components::{
        audioupload::AudioUpload, cardref::CardRefRender, dropdown::DropComponent, CardRef,
        DropDownMenu,
    },
    overlays::{card_selector::MyClosure, cardviewer::TempNode, OverlayEnum},
    APP, IS_SHORT,
};

#[derive(PartialEq, Clone)]
pub struct BackPut {
    pub text: Signal<String>,
    pub dropdown: DropDownMenu<BackOpts>,
    pub ref_card: CardRef,
    pub audio: Signal<Option<Audio>>,
}

#[component]
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
                            filter: ref_card.filter.clone(),
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
            audio: Signal::new_in_scope(Default::default(), ScopeId(3)),
            dropdown: DropDownMenu::new(BackOpts::iter(), Some(backopt)),
            ref_card,
        }
    }

    pub fn with_deselect(mut self, f: MyClosure) -> Self {
        self.ref_card = self.ref_card.with_closure(f);
        self
    }

    pub fn with_closure(mut self, f: MyClosure) -> Self {
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
}

#[derive(Default, Copy, Clone, Debug, Serialize, Deserialize, EnumIter, PartialEq)]
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
