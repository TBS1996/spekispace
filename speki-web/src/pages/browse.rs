use dioxus::prelude::*;
use tracing::info;

use crate::{
    overlays::card_selector::{CardSelector, CardSelectorRender},
    overlays::Overender,
};

#[derive(Clone)]
pub struct BrowseState {
    pub browse_page: CardSelector,
}

static BROWSE_STATE: GlobalSignal<BrowseState> = Signal::global(BrowseState::new);

#[component]
pub fn Browse() -> Element {
    let browse_state = BROWSE_STATE.cloned();
    let overlay = browse_state.browse_page.overlay.clone();

    rsx! {
        Overender {
            overlay,
            root: rsx!{
                CardSelectorRender {
                    title: browse_state.browse_page.title.clone(),
                    search: browse_state.browse_page.search.clone(),
                    on_card_selected: browse_state.browse_page.on_card_selected.clone(),
                    cards: browse_state.browse_page.cards.clone(),
                    allow_new: browse_state.browse_page.allow_new.clone(),
                    done: browse_state.browse_page.done.clone(),
                    dependents: browse_state.browse_page.dependents.clone(),
                    allowed_cards: browse_state.browse_page.allowed_cards.clone(),
                    filtereditor: browse_state.browse_page.filtereditor.clone(),
                    filtermemo: browse_state.browse_page.filtermemo.clone(),
                    overlay: browse_state.browse_page.overlay.clone(),
                    collection: browse_state.browse_page.collection,
                }
            }
         }
    }
}

impl BrowseState {
    pub fn new() -> Self {
        info!("creating browse state!");

        let browse_page = CardSelector::new(true, vec![]).with_title("Browse cards".to_string());

        Self { browse_page }
    }
}
