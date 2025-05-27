use dioxus::prelude::*;

use crate::{
    overlays::cardviewer::{CardViewer, CardViewerRender},
    overlays::Overender,
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

    rsx! {
        Overender {
            overlay: selv.viewer.overlay.clone(),
            root:
            rsx! {
                CardViewerRender {
                    editor: selv.viewer.editor.clone(),
                    dependents: selv.viewer.dependents.clone(),
                    graph: selv.viewer.graph.clone(),
                    save_hook: selv.viewer.save_hook.clone(),
                    is_done: selv.viewer.is_done.clone(),
                    old_card: selv.viewer.old_card.clone(),
                    old_meta: selv.viewer.old_meta.clone(),
                    tempnode: selv.viewer.tempnode.clone(),
                    overlay: selv.viewer.overlay.clone(),
                }
            }
        }
    }
}
