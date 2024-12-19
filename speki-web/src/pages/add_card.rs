use dioxus::prelude::*;

use crate::components::GraphRep;
use crate::overlays::cardviewer::CardViewer;
use crate::utils::CardEntries;
use crate::App;
use crate::Komponent;

#[derive(Clone)]
pub struct AddCardState {
    viewer: CardViewer,
}

impl AddCardState {
    pub fn new(graph: GraphRep, app: App, entries: CardEntries) -> Self {
        Self {
            viewer: CardViewer::new(graph, app, entries),
        }
    }
}

#[component]
pub fn Add() -> Element {
    let selv = use_context::<AddCardState>();

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
