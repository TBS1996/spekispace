use std::rc::Rc;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use dioxus::prelude::*;
use speki_core::{AnyType, Card};
use speki_web::BrowsePage;
use tracing::info;

use crate::App;
use crate::{
    components::{card_selector, display_card},
    graph::GraphRep,
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
    pub selected_card: Signal<BrowsePage>,
    pub cards: Signal<Vec<CardEntry>>,
    pub front_input: Signal<String>,
    pub back_input: Signal<String>,
    pub search: Signal<String>,
    pub graph: Signal<GraphRep>,
    pub refreshed: Arc<AtomicBool>,
}

impl BrowseState {
    pub fn new() -> Self {
        info!("creating browse state!");
        let selv = Self {
            selected_card: Default::default(),
            cards: Default::default(),
            front_input: Default::default(),
            back_input: Default::default(),
            search: Default::default(),
            graph: Signal::new(GraphRep::init("browcy".to_string())),
            refreshed: Default::default(),
        };

        speki_web::set_signal(selv.selected_card.clone());
        selv
    }

    fn maybe_refresh(&self) {
        info!("maybe refresh");
        if !self.refreshed.load(Ordering::SeqCst) {
            let selv = self.clone();
            spawn(async move {
                selv.refresh_cards().await;
                selv.refreshed.store(true, Ordering::SeqCst);
            });
        }
    }

    pub async fn refresh_cards(&self) {
        info!("refreshing cards");
        let app = use_context::<App>();
        let mut out = vec![];
        for card in app.as_ref().load_all_cards().await {
            out.push(CardEntry {
                front: card.print().await,
                card,
            });
        }

        self.cards.clone().set(out);
    }
}

impl Default for BrowseState {
    fn default() -> Self {
        Self::new()
    }
}

#[component]
pub fn Browse() -> Element {
    let browse_state = use_context::<BrowseState>();
    browse_state.maybe_refresh();
    let app = use_context::<App>();
    let selected_card = browse_state.selected_card.clone();
    let display_cyto = use_signal(|| false);

    let sel = selected_card.clone();
    let _display = display_cyto.clone();
    use_effect(move || {
        let mut display = _display.clone();
        let should_display = match sel.cloned() {
            BrowsePage::Browse => false,
            BrowsePage::View(_) => true,
            BrowsePage::SetDependency(_) => false,
        };

        display.set(should_display);
    });

    let sel = selected_card.clone();
    let _app = app.clone();
    use_effect(move || {
        let _ = sel.cloned();
        spawn(async move {
            let card = match sel.cloned() {
                BrowsePage::Browse => return,
                BrowsePage::View(card) => card,
                BrowsePage::SetDependency(card) => card,
            };

            info!("selected card: {card:?}");

            let raw = card.to_raw();
            let front = raw.data.front.unwrap_or_default();
            let back = raw.data.back.unwrap_or_default().to_string();
            browse_state.front_input.clone().set(front);
            browse_state.back_input.clone().set(back);
        });
    });

    let _selected = browse_state.selected_card.clone();
    let view_closure = move |card: Arc<Card<AnyType>>| {
        info!("view closure :D");
        let b = BrowsePage::View(card.clone());
        speki_web::set_browsepage(b);
        let mut sel = _selected.clone();
        sel.set(BrowsePage::View(card));
    };

    let _dep_closure = browse_state.selected_card.clone();
    let dep_closure = move |sel_card: Arc<Card<AnyType>>| {
        let mut sel = _selected.clone();
        info!("dep closure selected: {sel:?}");

        let current_card = match sel.cloned() {
            BrowsePage::Browse => return,
            BrowsePage::View(card) => card,
            BrowsePage::SetDependency(card) => card,
        };

        spawn(async move {
            let mut card = (*current_card).clone();
            let b = BrowsePage::View(Arc::new(card.clone()));
            speki_web::set_browsepage(b);
            info!("settting dependency..");
            card.add_dependency(sel_card.id).await;
            info!("refreshing card");
            let card = card.refresh().await;
            info!("setting to view");
            sel.set(BrowsePage::View(card));
        });
    };

    rsx! {
        crate::nav::nav {}

        match selected_card() {
            BrowsePage::View(_) => rsx! { display_card::display_card {} },
            BrowsePage::Browse => rsx !{ card_selector::card_selector {
                title: "browse cards".to_string(),
                search: browse_state.search.clone(),
                on_card_selected: Rc::new(view_closure),
                cards: browse_state.cards.clone(),
            }},
            BrowsePage::SetDependency(_) => rsx !{ card_selector::card_selector {
                title: "set dependency".to_string(),
                search: browse_state.search.clone(),
                on_card_selected: Rc::new(dep_closure),
                cards: browse_state.cards.clone(),
            }},
        }
    }
}
