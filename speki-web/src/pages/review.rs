use std::rc::Rc;
use std::sync::{Arc, Mutex};

use dioxus::prelude::*;
use speki_core::{AnyType, Card};
use speki_dto::CardId;
use speki_dto::Recall;
use tracing::info;

use crate::components::Komponent;
use crate::overlays::cardviewer::CardViewer;
use crate::overlays::Overlay;
use crate::{components::GraphRep, APP, DEFAULT_FILTER};
use crate::{IS_SHORT, OVERLAY};

static REVIEW_STATE: GlobalSignal<ReviewState> = Signal::global(ReviewState::new);

#[component]
pub fn Review() -> Element {
    let review = REVIEW_STATE.cloned();
    let rev2 = review.clone();
    let card = review.card.clone();
    let _card = card.clone();
    let reviewing = use_memo(move || _card().is_some());
    let filter_sig = review.filter.clone();

    let log_event = move |event: Rc<KeyboardData>| {
        let _review = review.clone();
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

    rsx! {
        div {
            id: "receiver",
            class: "h-full w-full",
            tabindex: 0,
            onkeydown: move |event| log_event(event.data()),

            if reviewing() {
              { rev2.render_queue() }
            } else {
                { review_start(filter_sig) }
            }
        }
    }
}

fn recall_button(recall: Recall) -> Element {
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
                spawn(async move{
                    REVIEW_STATE.cloned().do_review(recall).await;
                });
            },
            "{label}"

        }
    }
}

fn review_start(mut filter: Signal<String>) -> Element {
    let mut editing = use_signal(|| false);

    let class = if IS_SHORT.cloned() {
        "flex flex-col items-center h-screen space-y-4 justify-center"
    } else {
        "flex flex-col items-start h-screen space-y-4 pl-32"
    };

    rsx! {
        div {
            class: "{class}",

            if *editing.read() {
                div{
                    input {
                        class: "bg-white w-full max-w-[1000px] border border-gray-300 rounded-md p-2 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                        value: "{filter}",
                        oninput: move |evt| filter.set(evt.value().clone()),
                    }
                }
            }

            div {
                class: "flex space-x-4 mt-6",

                button {
                    class: "px-6 py-2 text-lg font-bold text-white bg-gray-600 border-0 rounded hover:bg-gray-500 focus:outline-none",
                    onclick: move |_| {
                        info!("Starting review...");
                        spawn(async move {
                            REVIEW_STATE.cloned().start_review().await;
                        });
                    },
                    if editing() {
                        "Start review"
                    } else {
                        "Default Review"
                    }
                },

                if !*editing.read() {
                    button {
                        class: "px-4 py-2 text-sm font-medium text-gray-700 bg-gray-200 border border-gray-300 rounded hover:bg-gray-300 focus:outline-none",
                        onclick: move |_| {
                            editing.set(true);
                        },
                        "Custom"
                    }
                }

            }
        }
    }
}

fn review_buttons(mut show_backside: Signal<bool>) -> Element {
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
                    { recall_button(Recall::None) }
                    { recall_button(Recall::Late) }
                    { recall_button(Recall::Some) }
                    { recall_button(Recall::Perfect) }
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct ReviewState {
    pub card: Signal<Option<Card<AnyType>>>,
    pub queue: Arc<Mutex<Vec<CardId>>>,
    pub tot_len: Signal<usize>,
    pub pos: Signal<usize>,
    pub front: Signal<String>,
    pub back: Signal<String>,
    pub show_backside: Signal<bool>,
    pub filter: Signal<String>,
    pub graph: GraphRep,
    pub show_graph: Signal<bool>,
    pub dependencies: Signal<Vec<(String, Arc<Card<AnyType>>, Self)>>,
}

impl ReviewState {
    fn info_bar(&self) -> Element {
        let front = self.front.clone();
        let back = self.back.clone();
        let pos = self.pos.clone();
        let tot = self.tot_len.clone();
        let currcard = self.card.clone();
        let overlay = OVERLAY.cloned();
        let card = self.card.clone();
        let show_graph = self.show_graph.clone();
        let selv = self.clone();

        rsx! {
            div {
                class: "flex justify-start items-center w-full md:w-auto gap-5",

                div {
                    button {
                        onclick: move |_| {
                            card.clone().set(None);
                        },
                        "❌"
                    }

                }

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
                        let fun = move |card: Arc<Card<AnyType>>| {
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
                            overlay.set(Box::new(viewer));
                        });
                    },
                    "✏️"
                }


                button {
                    class: "cursor-pointer text-gray-500 hover:text-gray-700",
                    onclick: move |_| {
                        let to_show = !show_graph.cloned();
                        show_graph.clone().set(to_show);

                        if to_show {
                            if let Some(card) = selv.card.cloned(){
                                selv.graph.new_set_card(card.into());
                            }

                        }

                    },
                    "toggle graph"
                }
            }
        }
    }

    fn card_sides(&self) -> Element {
        let front = self.front.clone();
        let back = self.back.clone();
        let show_backside = self.show_backside.clone();

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
                    { review_buttons(show_backside) }
                }
            }
        }
    }

    pub fn render_queue(&self) -> Element {
        let selv = self.clone();
        let graph = self.graph.clone();
        let graph_toggle = self.show_graph.clone();
        let deps = self.dependencies.clone();

        let show_graph = if self.show_backside.cloned() {
            "opacity-100 visible"
        } else {
            "opacity-0 invisible"
        };

        rsx! {
            div {
                class: "h-full w-full flex flex-col",

                div {
                    class: "flex-none w-full",
                    { selv.info_bar() }
                }

                div {
                    class: "flex flex-col md:flex-row w-full h-full overflow-hidden",

                    div {
                        class: "flex-1 w-full md:w-1/2 box-border order-1 md:order-2 relative",
                        style: "min-height: 0; flex-grow: 1;",

                        if graph_toggle() {
                            div {
                                class: "{show_graph} absolute top-0 left-0 w-full h-full",
                                { graph.render() }
                            }
                        } else {
                            div {
                                class: "flex flex-col {show_graph} absolute top-0 left-0 w-1/2 h-auto bg-white p-2 shadow-md rounded-md overflow-y-auto",
                                h4 {
                                    class: "font-bold mb-2",
                                    "Dependencies"
                                }
                                for (name, card, selv) in deps() {
                                    button {
                                        class: "mb-1 p-1 bg-gray-100 rounded-md text-left",
                                        onclick: move|_|{
                                            let selv = selv.clone();
                                            let card = card.clone();
                                            spawn(async move{
                                                let fun: Box<dyn Fn(Arc<Card<AnyType>>)> = Box::new(move |_: Arc<Card<AnyType>>| {
                                                    let selv = selv.clone();
                                                    spawn(async move{
                                                        selv.refresh().await;
                                                    });
                                                });

                                                let viewer = CardViewer::new_from_card(card, Default::default()).await.with_hook(Arc::new(fun));
                                                OVERLAY.write().set(Box::new(viewer));

                                            });
                                        },
                                        "{name}"
                                    }
                                }
                            }
                        }
                    }

                    div {
                        class: "flex-none w-full md:w-1/2 p-4 box-border overflow-y-auto overflow-x-hidden order-2 md:order-1",
                        style: "min-height: 0; max-height: 100%;",
                        { self.card_sides() }
                    }
                }
            }
        }
    }

    pub fn new() -> Self {
        Self {
            card: Default::default(),
            queue: Default::default(),
            tot_len: Default::default(),
            pos: Default::default(),
            front: Default::default(),
            back: Default::default(),
            show_backside: Default::default(),
            filter: Signal::new(DEFAULT_FILTER.to_string()),
            graph: Default::default(),
            show_graph: Default::default(),
            dependencies: Default::default(),
        }
    }

    pub async fn start_review(&mut self) {
        info!("refreshing..");
        let filter = Some(self.filter.cloned());
        let mut cards = APP.read().load_non_pending(filter.clone()).await;
        let pending = APP.read().load_pending(filter).await;
        cards.extend(pending);

        info!("review cards loaded");
        self.tot_len.clone().set(cards.len());
        {
            info!("setting queue");
            let mut lock = self.queue.lock().unwrap();
            *lock = cards;
            info!("queue was set");
        }
        self.next_card().await;
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
        self.tot_len - self.queue.lock().unwrap().len()
    }

    async fn refresh(&self) {
        info!("refreshing!");
        if let Some(card) = self.card.cloned() {
            let card = APP.read().load_card(card.id).await;
            self.graph.new_set_card(card.clone().into());

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
        let card = self.queue.lock().unwrap().pop();
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

        info!("card set: {:?}", card);
        self.card.clone().set(card.map(Arc::unwrap_or_clone));
        self.pos.clone().set(self.current_pos());
        self.show_backside.set(false);
        self.refresh().await;
    }
}
