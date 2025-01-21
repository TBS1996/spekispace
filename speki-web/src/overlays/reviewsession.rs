use dioxus::prelude::*;
use std::{rc::Rc, sync::Arc};

use speki_core::{card::CardId, cardfilter::CardFilter, recall_rate::Recall, Card};
use tracing::info;

use crate::{
    components::Komponent,
    overlays::{
        card_selector::{CardSelector, MyClosure},
        cardviewer::CardViewer,
    },
    pages::{Overender, OverlayEnum},
    APP,
};

use super::Overlay;

fn recall_button(recall: Recall, rev_state: ReviewState) -> Element {
    let label = match recall {
        Recall::None => "😡",
        Recall::Late => "😠",
        Recall::Some => "🙂",
        Recall::Perfect => "😁",
    };

    rsx! {
        button {
            class: "bg-white mt-6 inline-flex items-center justify-center text-white border-0 py-4 px-6 focus:outline-none hover:bg-gray-700 rounded md:mt-0 text-4xl leading-none",
            onclick: move |_| {
                let mut state = rev_state.clone();
                spawn(async move{
                    state.do_review(recall).await;
                });
            },
            "{label}"

        }
    }
}

fn review_buttons(mut show_backside: Signal<bool>, state: ReviewState) -> Element {
    rsx! {
        div {
            class: "flex flex-col items-center justify-center h-[68px]",

            if !show_backside() {
                button {
                    class: "inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base",
                    onclick: move |_| show_backside.set(true),
                    "show backside"
                }
            } else {
                div {
                    class: "flex gap-4 justify-center items-center",
                    { recall_button(Recall::None, state.clone()) }
                    { recall_button(Recall::Late, state.clone()) }
                    { recall_button(Recall::Some, state.clone()) }
                    { recall_button(Recall::Perfect, state.clone()) }
                }
            }
        }
    }
}

impl Overlay for ReviewState {
    fn is_done(&self) -> Signal<bool> {
        self.is_done.clone()
    }
}

impl Komponent for ReviewState {
    fn render(&self) -> Element {
        let selv = self.clone();
        let selv2 = self.clone();

        let log_event = move |event: Rc<KeyboardData>| {
            let _review = selv2.clone();
            spawn(async move {
                info!("reviewing..");
                let mut rev = _review.clone();
                let bck = rev.show_backside.cloned();
                match event.key().to_string().as_str() {
                    "1" if bck => rev.do_review(Recall::None).await,
                    "2" if bck => rev.do_review(Recall::Late).await,
                    "3" if bck => rev.do_review(Recall::Some).await,
                    "4" if bck => rev.do_review(Recall::Perfect).await,
                    " " => rev.show_backside.set(true),
                    _ => {}
                }
            });
        };

        let overlay = self.overlay.clone();
        rsx! {
            Overender {
                overlay,
                root: rsx! {
                    div {
                        class: "h-full w-full flex flex-col",
                        id: "receiver",
                        tabindex: 0,
                        onkeydown: move |event| log_event(event.data()),

                        div {
                            class: "flex-none w-full",
                            { selv.info_bar() }
                        }

                        div {
                            class: "flex flex-col md:flex-row w-full h-full overflow-hidden",

                            div {
                                class: "flex-1 w-full md:w-1/2 box-border order-1 md:order-2 relative",
                                style: "min-height: 0; flex-grow: 1;",
                                { selv.render_dependencies() }
                            }

                            div {
                                class: "flex-none w-full md:w-1/2 p-4 box-border overflow-y-auto overflow-x-hidden order-2 md:order-1",
                                style: "min-height: 0; max-height: 100%;",
                                { self.card_sides() }
                            }
                        }
                    }
                }

            },
        }
    }
}

#[derive(Clone, Debug)]
pub struct ReviewState {
    pub card: Signal<Option<Card>>,
    pub queue: Signal<Vec<CardId>>,
    pub tot_len: Signal<usize>,
    pub front: Signal<String>,
    pub back: Signal<String>,
    pub show_backside: Signal<bool>,
    pub dependencies: Signal<Vec<(String, Arc<Card>, Self)>>,
    pub is_done: Signal<bool>,
    pub overlay: Signal<Option<OverlayEnum>>,
}

impl ReviewState {
    pub async fn new_with_filter(cards: Vec<Arc<Card>>, filter: CardFilter) -> Self {
        let mut filtered = vec![];

        for card in cards {
            if filter.filter(card.clone()).await {
                filtered.push(card);
            }
        }

        Self::new(filtered).await
    }

    pub async fn new(cards: Vec<Arc<Card>>) -> Self {
        let mut selv = Self {
            card: Signal::new_in_scope(Default::default(), ScopeId::APP),
            tot_len: Signal::new_in_scope(Default::default(), ScopeId::APP),
            front: Signal::new_in_scope(Default::default(), ScopeId::APP),
            back: Signal::new_in_scope(Default::default(), ScopeId::APP),
            show_backside: Signal::new_in_scope(Default::default(), ScopeId::APP),
            dependencies: Signal::new_in_scope(Default::default(), ScopeId::APP),
            is_done: Signal::new_in_scope(Default::default(), ScopeId::APP),
            queue: Signal::new_in_scope(Default::default(), ScopeId::APP),
            overlay: Default::default(),
        };

        selv.start_review(cards).await;

        selv
    }

    pub async fn start_review(&mut self, cards: Vec<Arc<Card>>) {
        info!("start review for {} cards", cards.len());

        let mut thecards = vec![];

        for card in cards {
            thecards.push(card.id());
        }

        info!("review cards loaded!: so many cards: {}", thecards.len());
        self.tot_len.clone().set(thecards.len());
        self.queue.set(thecards);
        info!("queue was set");
        self.next_card().await;
    }

    fn info_bar(&self) -> Element {
        let front = self.front.clone();
        let back = self.back.clone();
        let tot = self.tot_len.clone();
        let currcard = self.card.clone();
        let selv2 = self.clone();
        let overlay = self.overlay.clone();
        let pos = self.current_pos();

        rsx! {
            div {
                class: "flex justify-start items-center w-full md:w-auto gap-5",
                h2 {
                    class: "text-2xl text-gray-700",
                    "{pos}/{tot}"
                }


                button {
                    class: "cursor-pointer text-gray-500 hover:text-gray-700",
                    onclick: move |_| {
                        let Some(card) = currcard.cloned() else {
                            return;
                        };


                        let front = front.clone();
                        let back = back.clone();
                        let fun = move |card: Arc<Card>| {
                            spawn(async move{
                                let f = card.print().await;
                                let b = card
                                    .display_backside()
                                    .await
                                    .unwrap_or_else(|| "___".to_string());
                                front.clone().set(f);
                                back.clone().set(b);

                                currcard.clone().set(Some(Arc::unwrap_or_clone(card)));
                            });

                        };

                        let overlay = overlay.clone();
                        spawn(async move {
                            let viewer = CardViewer::new_from_card(Arc::new(card), Default::default()).await.with_hook(Arc::new(Box::new(fun)));
                            let viewer = OverlayEnum::CardViewer(viewer);
                            overlay.clone().set(Some(viewer));
                        });
                    },
                    "✏️"
                }
                { selv2.suspend() }
            }
        }
    }

    fn card_sides(&self) -> Element {
        let front = self.front.clone();
        let back = self.back.clone();
        let show_backside = self.show_backside.clone();
        let selv = self.clone();

        let backside_visibility_class = if show_backside() {
            "opacity-100 visible"
        } else {
            "opacity-0 invisible"
        };

        rsx! {
            div {
                class: "flex flex-col items-center w-full",

                p {
                    class: "text-lg text-gray-800 text-center mb-10",
                    "{front}"
                }

                div {
                    class: "flex flex-col w-full items-center",

                    div {
                        class: "w-2/4 h-0.5 bg-gray-300",
                        style: "margin-top: 4px; margin-bottom: 12px;",
                    }

                    p {
                        class: "text-lg text-gray-700 text-center mb-4 {backside_visibility_class}",
                        "{back}"
                    }
                }

                div {
                    class: "w-full flex justify-center items-center",
                    { review_buttons(show_backside, selv) }
                }
            }
        }
    }

    fn suspend(&self) -> Element {
        let Some(card) = self.card.cloned() else {
            return rsx! {};
        };

        let is_suspended = card.is_suspended();
        let txt = if is_suspended { "unsuspend" } else { "suspend" };
        let selv = self.clone();

        rsx! {
            button {
                class: "mt-2 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base md:mt-0",
                onclick: move |_| {
                    let card = card.clone();
                    let mut selv = selv.clone();
                    spawn(async move {
                        let mut card = card;
                        card.set_suspend(true).await;
                        selv.next_card().await;
                    });
                },
                "{txt}"
            }
        }
    }

    fn render_dependencies(&self) -> Element {
        let show_graph = if self.show_backside.cloned() {
            "opacity-100 visible"
        } else {
            "opacity-0 invisible"
        };

        let deps = self.dependencies.clone();
        let Some(card) = self.card.cloned() else {
            return rsx! {"no card??"};
        };

        let selv = self.clone();
        rsx! {
            div {
                class: "flex flex-col {show_graph} absolute top-0 left-0 w-1/2 h-auto bg-white p-2 shadow-md rounded-md overflow-y-auto",

                div {
                    class: "flex items-center justify-between mb-2",

                    h4 {
                        class: "font-bold",
                        "Dependencies"
                    }

                        button {
                            class: "p-1 hover:bg-gray-200 hover:border-gray-400 border border-transparent rounded-md transition-colors",
                            onclick: move |_| {
                                let currcard = card.clone();

                                let selv = selv.clone();
                                let selv2 = selv.clone();
                                let fun = MyClosure(Arc::new(Box::new(move |card: Arc<Card>| {
                                    let selv = selv.clone();
                                    let mut old_card = currcard.clone();
                                    spawn(async move {
                                        old_card.add_dependency(card.id()).await;
                                        selv.refresh().await;
                                    });
                                })));

                                spawn(async move {
                                    let props = CardSelector::dependency_picker(fun).await;
                                    selv2.overlay.clone().set(Some(OverlayEnum::CardSelector(props)));
                                });
                            },
                            "➕"
                        }
                    }

                for (name, card, selv) in deps() {
                    button {
                        class: "mb-1 p-1 bg-gray-100 rounded-md text-left",
                        onclick: move|_|{
                            let selv = selv.clone();
                            let selv2 = selv.clone();
                            let card = card.clone();
                            spawn(async move{
                                let fun: Box<dyn Fn(Arc<Card>)> = Box::new(move |_: Arc<Card>| {
                                    let selv = selv.clone();
                                    spawn(async move{
                                        selv.refresh().await;
                                    });
                                });

                                let viewer = CardViewer::new_from_card(card, Default::default()).await.with_hook(Arc::new(fun));
                                selv2.overlay.clone().set(Some(OverlayEnum::CardViewer(viewer)));
                            });
                        },
                        "{name}"
                    }
                }
            }
        }
    }

    pub async fn do_review(&mut self, review: Recall) {
        info!("do review");
        self.make_review(review).await;
        self.next_card().await;
    }

    async fn make_review(&self, recall: Recall) {
        info!("make review");
        self.card.cloned().unwrap().add_review(recall).await;
    }

    fn current_pos(&self) -> usize {
        self.tot_len - self.queue.read().len()
    }

    async fn refresh(&self) {
        info!("refreshing!");
        if let Some(card) = self.card.cloned() {
            let card = APP.read().load_card(card.id()).await;

            let mut deps = vec![];
            for dep in &card.dependency_ids().await {
                let dep = APP.read().load_card(*dep).await;
                let s = dep.print().await;
                deps.push((s, dep, self.clone()));
            }

            self.dependencies.clone().set(deps);
            self.card.clone().set(Some(Arc::unwrap_or_clone(card)))
        } else {
            self.dependencies.clone().write().clear();
        }
    }

    async fn next_card(&mut self) {
        let card = self.queue.write().pop();
        let card = match card {
            Some(id) => {
                let card = APP.read().load_card(id).await;
                let front = card.print().await;
                let back = card
                    .display_backside()
                    .await
                    .unwrap_or_else(|| "___".to_string());

                self.front.clone().set(front);
                self.back.clone().set(back);
                Some(card)
            }
            None => None,
        };

        if card.is_none() {
            self.is_done.clone().set(true);
            return;
        }

        info!("card set: {:?}", card);
        self.card.clone().set(card.map(Arc::unwrap_or_clone));
        self.show_backside.set(false);
        self.refresh().await;
    }
}
