use std::rc::Rc;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use dioxus::prelude::*;
use speki_core::{AnyType, Card};
use speki_web::BrowsePage;
use tracing::info;

use crate::overlays::card_selector::CardSelectorProps;
use crate::{graph::GraphRep, overlays::card_selector, App};
use crate::{OverlayManager, PopTray, Popup};

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
    pub browse_menu: Signal<BrowsePage>,
    pub front_input: Signal<String>,
    pub back_input: Signal<String>,
    pub graph: Signal<GraphRep>,
    pub refreshed: Arc<AtomicBool>,
    pub browse_page: CardSelectorProps,
}

impl BrowseState {
    pub fn new() -> Self {
        info!("creating browse state!");
        let selected_card: Signal<BrowsePage> = Default::default();
        speki_web::set_signal(selected_card.clone());

        let props = card_selector::CardSelectorProps {
            title: "browse cards".to_string(),
            search: Default::default(),
            on_card_selected: Rc::new(Self::view_closure(selected_card.clone())),
            cards: Default::default(),
            done: Default::default(),
        };

        let selv = Self {
            browse_menu: selected_card,
            front_input: Default::default(),
            back_input: Default::default(),
            graph: Signal::new(GraphRep::init("browcy".to_string())),
            refreshed: Default::default(),
            browse_page: props,
        };

        selv
    }

    fn view_closure(selected: Signal<BrowsePage>) -> impl Fn(Arc<Card<AnyType>>) {
        move |card: Arc<Card<AnyType>>| {
            info!("view closure :D");
            let b = BrowsePage::View(card.clone());
            speki_web::set_browsepage(b);
            let mut sel = selected.clone();
            sel.set(BrowsePage::View(card));
        }
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

        self.browse_page.cards.clone().set(out);
    }

    fn view_card(&self, card: Arc<Card<AnyType>>) -> Element {
        info!("rendering display_card");
        let app = use_context::<App>();
        let mut selected_card = self.browse_menu.clone();

        let graphing = self.graph.clone();
        if let Some(browse) = speki_web::take_browsepage() {
            info!("set some!!");
            selected_card.set(browse);
            let _app = app.clone();
            let _card = card.clone();
            spawn(async move {
                graphing.read().set_card(_card).await;
            });
        } else {
            tracing::trace!("nope no set");
        }

        let mut front_input = self.front_input.clone();
        let mut back_input = self.back_input.clone();

        let _card = card.clone();
        let _app = app.clone();
        let selv = self.clone();
        let selv2 = self.clone();
        let selv3 = self.clone();
        rsx! {
                input {
                    class: "w-full border border-gray-300 rounded-md p-2 mb-4 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                    value: "{front_input}",
                    oninput: move |evt| front_input.set(evt.value()),
                }
                input {
                    class: "w-full border border-gray-300 rounded-md p-2 mb-4 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                    value: "{back_input}",
                    oninput: move |evt| back_input.set(evt.value()),
                }

                div {
                    button {
                        class: "mt-6 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base md:mt-0",
                        onclick: move |_| {
                            let selected_card = selv.browse_menu.clone();
                            let fun = move |card: Arc<Card<AnyType>>| {
                                selected_card.clone().set(BrowsePage::View(card));

                            };

                            let props = CardSelectorProps {
                                title: "set dependency".to_string(),
                                search: selv.browse_page.search.clone(),
                                on_card_selected: Rc::new(fun),
                                cards: selv.browse_page.cards.clone(),
                                done: Default::default(),
                            };
                            let pop: Popup = Box::new(props);
                            use_context::<OverlayManager>().set(pop);



                        },
                        "set dependency"
                    }
                    button {
                        class: "mt-6 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base md:mt-0",
                        onclick: move |_| {
                            selv2.browse_menu.clone().set(BrowsePage::Browse);
                        },
                        "go back"
                    }
                    button {
                        class: "mt-6 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base md:mt-0",
                        onclick: move |_| {
                            let value = card.clone();
                            let mut selv = selv3.clone();
                            spawn(async move {
                                let front = front_input.cloned();
                                let back = back_input.cloned();
                                let mut card = (*value).clone();
                                let mut raw = card.to_raw();
                                raw.data.front = Some(front);
                                raw.data.back = Some(back.into());

                                info!("raw stuff: {raw:?}");

                                card.update_with_raw(raw).await;

                                selv.browse_menu.set(BrowsePage::Browse);
                                selv.refresh_cards().await;

                                });
                        },
                        "save"
                    }
                }
            { self.graph.read().render() }
        }
    }

    fn set_selected(&self) {
        let sel = self.browse_menu.clone();
        let browse_state = self.clone();
        use_effect(move || {
            let _ = sel.cloned();
            spawn(async move {
                let card = match sel.cloned() {
                    BrowsePage::Browse => return,
                    BrowsePage::View(card) => card,
                };

                info!("selected card: {card:?}");

                let raw = card.to_raw();
                let front = raw.data.front.unwrap_or_default();
                let back = raw.data.back.unwrap_or_default().to_string();
                browse_state.front_input.clone().set(front);
                browse_state.back_input.clone().set(back);
            });
        });
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
    let selected_card = browse_state.browse_menu.clone();
    browse_state.set_selected();

    rsx! {
        match selected_card() {
            BrowsePage::View(card) => { browse_state.view_card(card) },
            BrowsePage::Browse => { browse_state.browse_page.render() }
        }
    }
}
