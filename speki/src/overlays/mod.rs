pub mod card_selector;
pub mod cardviewer;
pub mod notice;
pub mod reviewsession;
pub mod textinput;
pub mod uploader;

use crate::{
    append_overlay,
    overlays::{
        card_selector::CardSelector, cardviewer::CardViewer, notice::NoticeRender,
        reviewsession::ReviewState, textinput::TextInputRender,
    },
    pop_overlay, set_overlay, APP,
};
use card_selector::CardSelectorRender;
use cardviewer::CardViewerRender;
use dioxus::prelude::*;
use nonempty::NonEmpty;
use reviewsession::ReviewRender;
use speki_core::card::CardId;
use std::{fmt::Debug, sync::Arc};

#[derive(Clone)]
pub struct OverlayChoice {
    pub display: String,
    pub overlay: Arc<Box<dyn Fn() -> Option<OverlayEnum>>>,
}

impl PartialEq for OverlayChoice {
    fn eq(&self, other: &Self) -> bool {
        self.display == other.display
    }
}

impl Eq for OverlayChoice {}

#[component]
pub fn OverlaySelectorRender(title: String, choices: Vec<OverlayChoice>) -> Element {
    rsx! {
        div {
            class: "flex flex-col items-center",

            p {
                class: "text-3xl font-bold text-center mb-4",
                "{title}"
            }

            for choice in choices {
                button {
                    class: "w-48 my-2 {crate::styles::READ_BUTTON}",
                    onclick: move |_| {
                        let new = (choice.overlay)();
                        set_overlay(new);
                    },
                    "{choice.display}"
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct OverlaySelector {
    pub title: String,
    pub choices: Vec<OverlayChoice>,
    pub chosen: Option<Box<OverlayEnum>>,
}

#[derive(Clone)]
pub enum OverlayEnum {
    Review(ReviewState),
    CardViewer(CardViewer),
    CardSelector(CardSelector),
    OverlaySelector(OverlaySelector),
    Text {
        question: Arc<String>,
        input_value: Signal<String>,
        on_submit: Arc<Box<dyn Fn(String)>>,
    },
    Notice {
        text: String,
        button_text: String,
    },
}

impl OverlayEnum {
    pub fn append(self) {
        append_overlay(self);
    }

    pub fn new_review(thecards: NonEmpty<CardId>) -> Self {
        Self::Review(ReviewState::new(thecards))
    }

    pub fn new_edit_card(id: CardId) -> Self {
        let card = APP.read().load_card(id);
        Self::CardViewer(CardViewer::new_from_card(card))
    }

    pub fn new_create_card() -> Self {
        Self::CardViewer(CardViewer::new())
    }

    pub fn new_text_input(q: String, hook: Arc<Box<dyn Fn(String)>>) -> Self {
        Self::Text {
            question: Arc::new(q),
            input_value: Signal::new_in_scope(Default::default(), ScopeId::APP),
            on_submit: hook,
        }
    }

    pub fn new_notice(text: impl AsRef<str>) -> Self {
        let text = text.as_ref().to_string();
        Self::Notice {
            text,
            button_text: "OK".to_string(),
        }
    }
}

impl Debug for OverlayEnum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Review(_) => f.debug_tuple("Review").finish(),
            Self::CardViewer(_) => f.debug_tuple("card viewer").finish(),
            Self::CardSelector(_) => f.debug_tuple("card selector").finish(),
            Self::OverlaySelector(_) => f.debug_tuple("overlay selector").finish(),
            Self::Notice { .. } => f.debug_tuple("notice").finish(),
            Self::Text { .. } => f.debug_tuple("text").finish(),
        }
    }
}

/// Handles recursive overlays
///
/// Takes in a view of an overlay, and an optional overlay.
/// todo: should overlay be a memo or something of done signal ? i mean, when we press done, overlay should be closed? or is that unceessary
/// abstraction?
#[component]
pub fn Overender(overlay: Signal<Option<Arc<OverlayEnum>>>, root: Element) -> Element {
    rsx! {
        match overlay.cloned() {
            None => root,
            Some(elm) => {
                rsx!{
                    div {
                        button {
                            onclick: move |_| {
                                // Note that pressing X will close its parents overlay, which represents the current view.
                                pop_overlay();
                            },
                            "❌"
                        }

                        match Arc::unwrap_or_clone(elm) {
                            OverlayEnum::Text{ question, input_value, on_submit } => {
                                rsx!{
                                    TextInputRender {
                                        question: question.clone(),
                                        input_value: input_value.clone(),
                                        on_submit: move |val| {
                                            (on_submit)(val);
                                        },
                                    }
                                }
                            },
                            OverlayEnum::Review(elm) => rsx!{
                                ReviewRender {
                                    queue: elm.queue.clone(),
                                    card_id: match elm.queue.read().current() {
                                        Some(id) => id,
                                        None => {
                                            overlay.set(None);
                                            return root;
                                        },

                                    },
                                    show_backside: elm.show_backside.clone(),
                                    tot: elm.tot_len,
                                }
                            },
                            OverlayEnum::CardViewer(elm) => rsx!{
                                CardViewerRender {
                                    editor: elm.editor.clone(),
                                    save_hook: elm.save_hook.clone(),
                                    old_card: elm.old_card.clone(),
                                }
                            },
                            OverlayEnum::OverlaySelector(elm) => rsx! {
                                OverlaySelectorRender { title: elm.title.clone(), choices: elm.choices.clone()}
                            },

                            OverlayEnum::Notice{
                                text, button_text
                            } => {
                                rsx! {
                                    NoticeRender {text, button_text}
                                }
                            }

                            OverlayEnum::CardSelector(elm) => rsx!{
                                CardSelectorRender {
                                    title: elm.title.clone(),
                                    search: elm.search.clone(),
                                    on_card_selected: elm.on_card_selected.clone(),
                                    cards: elm.cards.clone(),
                                    allow_new: elm.allow_new.clone(),
                                    allowed_cards: elm.allowed_cards.clone(),
                                    filtereditor: elm.filtereditor.clone(),
                                    filtermemo: elm.filtermemo.clone(),
                                    collection: elm.collection.clone(),
                                    edit_collection: elm.edit_collection,
                                    instance_of: elm.instance_of.clone(),
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}
