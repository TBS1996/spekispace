use dioxus::prelude::*;

use crate::{
    components::Komponent,
    overlays::{cardviewer::CardViewer, uploader::Uploader},
    Route, OVERLAY,
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
        button {
            class: "px-4 py-1 bg-blue-500 text-white rounded hover:bg-blue-600",
            onclick: move |_| {
                OVERLAY.cloned().set(Box::new(Uploader::new()))

            },
            "upload from file"
        }
        { selv.viewer.render() }
    }
}
