use dioxus::prelude::*;

use crate::overlays::cardviewer::{CardViewer, CardViewerRender};

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
        CardViewerRender {
            title: selv.viewer.title.clone(),
            front: selv.viewer.front.clone(),
            back: selv.viewer.back.clone(),
            concept: selv.viewer.concept.clone(),
            dependencies: selv.viewer.dependencies.clone(),
            dependents: selv.viewer.dependents.clone(),
            graph: selv.viewer.graph.clone(),
            save_hook: selv.viewer.save_hook.clone(),
            is_done: selv.viewer.is_done.clone(),
            old_card: selv.viewer.old_card.clone(),
            old_meta: selv.viewer.old_meta.clone(),
            filter: selv.viewer.filter.clone(),
            tempnode: selv.viewer.tempnode.clone(),
            allowed_cards: selv.viewer.allowed_cards.clone(),
            overlay: selv.viewer.overlay.clone(),
        }
    }
}
