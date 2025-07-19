use dioxus::prelude::*;
use tracing::info;

use crate::{
    overlays::{
        card_selector::{CardSelector, CardSelectorRender},
        Overender,
    },
    APP, OVERLAY,
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

    let card_state = APP
        .read()
        .inner()
        .provider
        .cards
        .currently_applied_ledger_hash();
    if browse_state.prev_cardstate != card_state {
        *BROWSE_STATE.write() = BrowseState::new();
        browse_state = BROWSE_STATE.cloned();
    }

    let overlay = dbg!(OVERLAY.read().get());

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
                    allowed_cards: browse_state.browse_page.allowed_cards.clone(),
                    filtereditor: browse_state.browse_page.filtereditor.clone(),
                    filtermemo: browse_state.browse_page.filtermemo.clone(),
                    collection: browse_state.browse_page.collection,
                    edit_collection: browse_state.browse_page.edit_collection,
                    instance_of: browse_state.browse_page.instance_of.clone(),
                    reviewable: true,
                }
            }
         }
    }
}

impl BrowseState {
    pub fn new() -> Self {
        info!("creating browse state!");

        let browse_page = CardSelector::new(true, vec![]).no_title();
        let prev_cardstate = APP
            .read()
            .inner()
            .provider
            .cards
            .currently_applied_ledger_hash();

        Self {
            browse_page,
            prev_cardstate,
        }
    }
}
