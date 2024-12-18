use dioxus::prelude::*;

use crate::{
    components::{GraphRep, Komponent},
    overlays::cardviewer::CardViewer,
    ADD_CARDS, APP, CARDS,
};

#[derive(Clone)]
pub struct AddCardState {
    viewer: CardViewer,
}

impl AddCardState {
    pub fn new(graph: GraphRep) -> Self {
        let entries = CARDS.cloned();
        let app = APP.read().clone();
        Self {
            viewer: CardViewer::new(graph, app, entries),
        }
    }
}

#[component]
pub fn Add() -> Element {
    let selv = ADD_CARDS.cloned();

    rsx! {
        div {
            style: "max-width: 500px; margin: 0 auto;",
            div {
                h1 {
                    class: "text-2xl font-bold text-gray-800 mb-6 text-center",
                    "Add Flashcard"
                }

                { selv.viewer.render() }
            }
        }
    }
}
