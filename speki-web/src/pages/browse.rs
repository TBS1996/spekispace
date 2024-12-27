use std::sync::Arc;

use dioxus::prelude::*;
use speki_core::{AnyType, Card};
use tracing::info;

use crate::{
    components::{GraphRep, Komponent},
    overlays::{card_selector::CardSelector, cardviewer::CardViewer},
    OVERLAY,
};

#[derive(Clone)]
pub struct BrowseState {
    pub browse_page: CardSelector,
}

static BROWSE_STATE: GlobalSignal<BrowseState> = Signal::global(BrowseState::new);

#[component]
pub fn Browse() -> Element {
    let browse_state = BROWSE_STATE.cloned();

    rsx! {
        { browse_state.browse_page.render() }
    }
}

#[derive(Clone)]
pub struct CardEntry {
    pub front: String,
    pub card: Arc<Card<AnyType>>,
}

impl CardEntry {
    pub async fn new(card: Arc<Card<AnyType>>) -> Self {
        Self {
            front: card.print().await,
            card,
        }
    }
}

fn overlay_card_viewer() -> Arc<Box<dyn Fn(Arc<Card<AnyType>>)>> {
    Arc::new(Box::new(move |card: Arc<Card<AnyType>>| {
        spawn(async move {
            let graph = GraphRep::new().with_hook(overlay_card_viewer());
            let viewer = CardViewer::new_from_card(card, graph).await;
            OVERLAY.cloned().replace(Box::new(viewer));
        });
    }))
}

impl BrowseState {
    pub fn new() -> Self {
        info!("creating browse state!");

        let browse_page = CardSelector {
            title: "browse cards".to_string(),
            cards: Default::default(),
            on_card_selected: overlay_card_viewer(),
            search: Default::default(),
            done: Default::default(),
            allow_new: false,
            filter: None,
            dependents: Default::default(),
        };

        Self { browse_page }
    }
}
