pub mod card_selector;
pub mod cardviewer;
pub mod reviewsession;
pub mod uploader;
//pub mod yesno;
//pub mod itemselector;
//pub mod textinput;

use crate::overlays::{
    card_selector::CardSelector, cardviewer::CardViewer, reviewsession::ReviewState,
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
pub fn OverlaySelectorRender(
    title: String,
    choices: Vec<OverlayChoice>,
    overlay: Signal<Option<OverlayEnum>>,
) -> Element {
    rsx! {
        p{"{title}"}

        div {
            class: "flex flex-col",

            for choice in choices {

                button {
                    onclick: move |_|{
                        let new = (choice.overlay)();
                        overlay.clone().set(new);
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
}

impl OverlayEnum {
    /// The overlay belonging to the overlay
    pub fn overlay(&self) -> Signal<Option<OverlayEnum>> {
        match self {
            OverlayEnum::Review(elm) => elm.overlay.clone(),
            OverlayEnum::OverlaySelector(_) => {
                Signal::new_in_scope(Default::default(), ScopeId::APP)
            }
            OverlayEnum::CardViewer(elm) => elm.overlay.clone(),
            OverlayEnum::CardSelector(elm) => elm.overlay.clone(),
        }
    }

    pub fn is_done(&self) -> bool {
        match self {
            OverlayEnum::Review(elm) => elm.is_done.cloned(),
            OverlayEnum::CardViewer(elm) => elm.is_done.cloned(),
            OverlayEnum::CardSelector(elm) => elm.done.cloned(),
            OverlayEnum::OverlaySelector(elm) => elm.chosen.is_some(),
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
        }
    }
}

/// Handles recursive overlays
///
/// Takes in a view of an overlay, and an optional overlay.
/// todo: should overlay be a memo or something of done signal ? i mean, when we press done, overlay should be closed? or is that unceessary
/// abstraction?
#[component]
pub fn Overender(overlay: Signal<Option<OverlayEnum>>, root: Element) -> Element {
    // If the given view has an overlay, before we render it we check if the overlay is marked as done
    // if marked as done, we set the overlay to none and in the next block will render the current view instead.
    if overlay.as_ref().is_some_and(|ol| ol.is_done()) {
        overlay.set(None);
    }

    rsx! {
        match overlay.cloned() {
            None => root,
            Some(elm) => {
                let theoverlay = elm.overlay(); // the overlay of the overlay, so to speak.
                let root = rsx!{
                    div {
                        button {
                            onclick: move |_| {
                                // Note that pressing X will close its parents overlay, which represents the current view.
                                overlay.clone().set(None);
                            },
                            "❌"
                        }

                        match elm {
                            OverlayEnum::Review(elm) => rsx!{
                                ReviewRender {
                                    queue: elm.queue.clone(),
                                    show_backside: elm.show_backside.clone(),
                                    tot: elm.tot_len,
                                    overlay: elm.overlay.clone(),
                                }
                            },
                            OverlayEnum::CardViewer(elm) => rsx!{
                                CardViewerRender {
                                    editor: elm.editor.clone(),
                                    dependents: elm.dependents.clone(),
                                    save_hook: elm.save_hook.clone(),
                                    is_done: elm.is_done.clone(),
                                    old_card: elm.old_card.clone(),
                                    old_meta: elm.old_meta.clone(),
                                    tempnode: elm.tempnode.clone(),
                                    overlay: elm.overlay.clone(),
                                }
                            },
                            OverlayEnum::OverlaySelector(elm) => rsx! {
                                OverlaySelectorRender { title: elm.title.clone(), choices: elm.choices.clone(), overlay: overlay.clone()  }
                            },
                            OverlayEnum::CardSelector(elm) => rsx!{
                                CardSelectorRender {
                                    title: elm.title.clone(),
                                    search: elm.search.clone(),
                                    on_card_selected: elm.on_card_selected.clone(),
                                    cards: elm.cards.clone(),
                                    allow_new: elm.allow_new.clone(),
                                    done: elm.done.clone(),
                                    dependents: elm.dependents.clone(),
                                    allowed_cards: elm.allowed_cards.clone(),
                                    filtereditor: elm.filtereditor.clone(),
                                    filtermemo: elm.filtermemo.clone(),
                                    overlay: elm.overlay.clone(),
                                    collection: elm.collection,
                                    edit_collection: elm.edit_collection,
                                }
                            },
                        }
                    }
                };

                rsx! { Overender {overlay: theoverlay, root} }
            }
        }
    }
}
