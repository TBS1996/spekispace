use dioxus::prelude::*;

use crate::{
    overlays::{
        cardviewer::{CardViewer, CardViewerRender},
        Overender,
    },
    OVERLAY,
};

static ADD_CARDS: GlobalSignal<AddCardState> = Signal::global(AddCardState::new);

#[derive(Clone)]
pub struct AddCardState {
    viewer: CardViewer,
}

impl AddCardState {
    pub fn new() -> Self {
        Self {
            viewer: CardViewer::new(),
        }
    }
}

#[component]
pub fn Add() -> Element {
    let selv = ADD_CARDS.cloned();
    let overlay = OVERLAY.read().get();

    rsx! {
        Overender {
            overlay,
            root:
                rsx! {
                    CardViewerRender {
                        editor: selv.viewer.editor.clone(),
                        save_hook: selv.viewer.save_hook.clone(),
                        old_card: selv.viewer.old_card.clone(),
                        show_import: true
                }
            }
        }
    }
}
