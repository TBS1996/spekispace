pub mod card_selector;
pub mod cardviewer;
pub mod notice;
pub mod reviewsession;
pub mod textinput;
pub mod uploader;

use crate::{
    append_overlay,
    overlays::{
        card_selector::CardSelector,
        cardviewer::CardViewer,
        notice::NoticeRender,
        reviewsession::ReviewState,
        textinput::{TextInput, TextInputRender},
    },
    pop_overlay, set_overlay,
};
use card_selector::CardSelectorRender;
use cardviewer::CardViewerRender;
use dioxus::prelude::*;
use reviewsession::ReviewRender;
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
    Text(TextInput),
    Notice { text: String, button_text: String },
}

impl OverlayEnum {
    pub fn append(self) {
        append_overlay(self);
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
            Self::Text(_) => f.debug_tuple("text").finish(),
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

                        match &*elm {
                            OverlayEnum::Text(elm) => {
                                rsx!{
                                    TextInputRender {
                                        question: elm.question.clone(),
                                        input_value: elm.input_value,
                                        on_submit: elm.on_submit.clone(),
                                        crud: elm.crud,
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
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}
