use std::{rc::Rc, sync::Arc};

use dioxus::prelude::*;
use speki_core::{AnyType, Card};
use tracing::info;

use crate::{
    components::{GraphRep, Komponent},
    overlays::{card_selector::CardSelector, cardviewer::CardViewer},
    BROWSE_STATE, CARDS, OVERLAY,
};

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

#[derive(Clone)]
pub struct BrowseState {
    pub browse_page: CardSelector,
}

impl BrowseState {
    pub fn new() -> Self {
        let entries = CARDS.cloned();
        info!("creating browse state!");

        let f = move |card: Arc<Card<AnyType>>| {
            spawn(async move {
                let x = CardViewer::new_from_card(card, GraphRep::init(None)).await;
                OVERLAY.cloned().set(Box::new(x));
            });
        };

        let browse_page = CardSelector {
            title: "browse cards".to_string(),
            search: Default::default(),
            on_card_selected: Rc::new(f),
            cards: entries.cards.clone(),
            done: Default::default(),
        };

        Self { browse_page }
    }
}

#[component]
pub fn Browse() -> Element {
    info!("browse!");
    let browse_state = BROWSE_STATE.cloned();

    rsx! {
        { browse_state.browse_page.render() }
    }
}
