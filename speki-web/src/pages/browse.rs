use std::sync::Arc;

use dioxus::prelude::*;
use speki_core::{AnyType, Card};
use tracing::info;

use crate::{components::Komponent, overlays::card_selector::CardSelector};

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

impl BrowseState {
    pub fn new() -> Self {
        info!("creating browse state!");

        let browse_page = CardSelector::new().with_title("Browse cards".to_string());

        Self { browse_page }
    }
}
