pub mod card_selector;
pub mod cardviewer;
pub mod colviewer;
pub mod itemselector;
pub mod reviewsession;
pub mod textinput;
pub mod uploader;
pub mod yesno;

use crate::overlays::{
    card_selector::CardSelector, cardviewer::CardViewer, colviewer::ColViewer,
    reviewsession::ReviewState, textinput::TextInput, yesno::Yesno,
};
use card_selector::CardSelectorRender;
use cardviewer::CardViewerRender;
use colviewer::ColViewRender;
use dioxus::prelude::*;
use itemselector::{ItemSelector, ItemSelectorRender};
use reviewsession::ReviewRender;
use speki_core::collection::Collection;
use std::fmt::Debug;
use textinput::TextInputRender;
use yesno::YesnoRender;

#[derive(Clone)]
pub enum OverlayEnum {
    Review(ReviewState),
    Colviewer(ColViewer),
    Text(TextInput),
    CardViewer(CardViewer),
    CardSelector(CardSelector),
    ColSelector(ItemSelector<Collection>),
    YesNo(Yesno),
}

impl OverlayEnum {
    /// The overlay belonging to the overlay
    pub fn overlay(&self) -> Signal<Option<OverlayEnum>> {
        match self {
            OverlayEnum::Review(elm) => elm.overlay.clone(),
            OverlayEnum::Colviewer(elm) => elm.overlay.clone(),
            OverlayEnum::Text(_) => Signal::new_in_scope(Default::default(), ScopeId::APP),
            OverlayEnum::YesNo(_) => Signal::new_in_scope(Default::default(), ScopeId::APP),
            OverlayEnum::ColSelector(_) => Signal::new_in_scope(Default::default(), ScopeId::APP),
            OverlayEnum::CardViewer(elm) => elm.overlay.clone(),
            OverlayEnum::CardSelector(elm) => elm.overlay.clone(),
        }
    }

    pub fn is_done(&self) -> bool {
        match self {
            OverlayEnum::Review(elm) => elm.is_done.cloned(),
            OverlayEnum::Colviewer(elm) => elm.done.cloned(),
            OverlayEnum::Text(elm) => elm.done.cloned(),
            OverlayEnum::CardViewer(elm) => elm.is_done.cloned(),
            OverlayEnum::CardSelector(elm) => elm.done.cloned(),
            OverlayEnum::YesNo(elm) => elm.done.cloned(),
            OverlayEnum::ColSelector(elm) => elm.done.cloned(),
        }
    }
}

impl Debug for OverlayEnum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Review(_) => f.debug_tuple("Review").finish(),
            Self::Colviewer(_) => f.debug_tuple("Colviewer").finish(),
            Self::Text(_) => f.debug_tuple("Text").finish(),
            Self::CardViewer(_) => f.debug_tuple("card viewer").finish(),
            Self::CardSelector(_) => f.debug_tuple("card selector").finish(),
            Self::YesNo(_) => f.debug_tuple("yesno").finish(),
            Self::ColSelector(_) => f.debug_tuple("col selector").finish(),
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
                            OverlayEnum::ColSelector(elm) => rsx!{
                                ItemSelectorRender{
                                    items: elm.items.clone(),
                                    on_selected: elm.on_selected.clone(),
                                    done: elm.done.clone(),
                                }
                            },
                            OverlayEnum::Review(elm) => rsx!{
                                ReviewRender {
                                    front: elm.front,
                                    back: elm.back.cloned().unwrap_or_default(),
                                    card: elm.card.cloned().unwrap().unwrap(),
                                    queue: elm.queue.clone(),
                                    show_backside: elm.show_backside.clone(),
                                    tot: elm.tot_len,
                                    overlay: elm.overlay.clone(),
                                    dependencies:elm.dependencies.clone(),
                                    filter: elm.filter.clone(),
                                }
                            },
                            OverlayEnum::Colviewer(elm) => rsx!{
                                ColViewRender{
                                    col: elm.col.clone(),
                                    colname:  elm.colname.clone(),
                                    done:  elm.done.clone(),
                                    entries: elm.entries.clone(),
                                    overlay: elm.overlay.clone(),
                                    addnew: elm.addnew.clone(),
                                }
                            },
                            OverlayEnum::Text(elm) => rsx!{
                                TextInputRender {
                                    question: elm.question.clone(),
                                    input_value: elm.input_value.clone(),
                                    done: elm.done.clone(),
                                    on_submit: elm.on_submit.clone(),
                                }
                            },
                            OverlayEnum::CardViewer(elm) => rsx!{
                                CardViewerRender {
                                    editor: elm.editor.clone(),
                                    dependents: elm.dependents.clone(),
                                    graph: elm.graph.clone(),
                                    save_hook: elm.save_hook.clone(),
                                    is_done: elm.is_done.clone(),
                                    old_card: elm.old_card.clone(),
                                    old_meta: elm.old_meta.clone(),
                                    tempnode: elm.tempnode.clone(),
                                    overlay: elm.overlay.clone(),
                                }
                            },
                            OverlayEnum::YesNo(elm) => rsx! {
                                YesnoRender {
                                    question: elm.question.clone(),
                                    done: elm.done.clone(),
                                    on_yes: elm.on_yes.clone(),
                                }
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
