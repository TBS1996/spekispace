use dioxus::prelude::*;

use crate::{components::Komponent, overlays::cardviewer::CardViewer};

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
        { selv.viewer.render() }
    }
}
