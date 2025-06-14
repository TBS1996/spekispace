use dioxus::prelude::*;
use tracing::info;

use crate::{
    overlays::{
        card_selector::{CardSelector, CardSelectorRender},
        Overender,
    },
    APP,
};

#[derive(Clone)]
pub struct BrowseState {
    pub browse_page: CardSelector,
    pub prev_cardstate: Option<String>,
}

static BROWSE_STATE: GlobalSignal<BrowseState> = Signal::global(BrowseState::new);

#[component]
pub fn Browse() -> Element {
    let mut browse_state = BROWSE_STATE.cloned();

    let card_state = APP.read().inner().provider.cards.state_hash();
    if browse_state.prev_cardstate != card_state {
        *BROWSE_STATE.write() = BrowseState::new();
        browse_state = BROWSE_STATE.cloned();
    }

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
                    edit_collection: browse_state.browse_page.edit_collection,
                }
            }
         }
    }
}

impl BrowseState {
    pub fn new() -> Self {
        info!("creating browse state!");

        let browse_page = CardSelector::new(true, vec![]).no_title();
        let prev_cardstate = APP.read().inner().provider.cards.state_hash();

        Self {
            browse_page,
            prev_cardstate,
        }
    }
}
