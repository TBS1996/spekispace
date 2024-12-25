use std::rc::Rc;
use std::sync::{Arc, Mutex};

use dioxus::prelude::*;
use speki_core::{AnyType, Card};
use speki_dto::CardId;
use speki_dto::Recall;
use tracing::info;

use crate::{components::GraphRep, APP, DEFAULT_FILTER};

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
        Recall::None => "☹️",
        Recall::Late => "🙁",
        Recall::Some => "🙂",
        Recall::Perfect => "😀",
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

    rsx! {
        div {
            class: "flex flex-col items-center h-screen space-y-4",

            if *editing.read() {
                div{
                    input {
                        class: "bg-white w-[700px] border border-gray-300 rounded-md p-2 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
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
                            REVIEW_STATE.cloned().refresh().await;
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

fn review_buttons() -> Element {
    rsx! {
        div {
            style: "display: flex; gap: 16px; justify-content: center; align-items: center;",
            { recall_button(Recall::None) }
            { recall_button(Recall::Late) }
            { recall_button(Recall::Some) }
            { recall_button(Recall::Perfect) }
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
}

impl ReviewState {
    pub fn render_queue(&self) -> Element {
        let back = self.back.clone();
        let front = self.front.clone();
        let pos = self.pos.clone();
        let tot = self.tot_len.clone();
        let mut show_backside = self.show_backside.clone();

        rsx! {
            div {
                class: "w-full max-w-4xl flex flex-col md:flex-row md:gap-8 items-start mt-12 px-4 md:px-0",
                div {
                    class: "flex justify-between items-center w-full md:w-auto",
                    h2 {
                        class: "text-2xl text-gray-700",
                        "{pos}/{tot}"
                    }
                }

                div {
                    class: "w-full flex flex-col items-center gap-6",

                    // Front text
                    p {
                        class: "text-lg text-gray-800 text-center",
                        "{front}"
                    }

                    // Fixed container for backside content and buttons
                    div {
                        class: "flex flex-col items-center w-full",
                        style: "min-height: 300px; display: flex; justify-content: flex-end; align-items: center;", // Reserve consistent height

                        if show_backside() {
                            // Backside content
                            p {
                                class: "text-lg text-gray-700 text-center mb-4",
                                "{back}"
                            }

                            // Review buttons
                            div {
                                class: "flex justify-center gap-4",
                                { review_buttons() }
                            }
                        } else {
                            // Show backside button aligned with review buttons
                            button {
                                class: "inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base",
                                onclick: move |_| show_backside.set(true),
                                "show backside"
                            }
                        }
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
        }
    }

    pub async fn refresh(&mut self) {
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

        if let Some(card) = card.as_ref() {
            self.graph.new_set_card(card.clone());
        }

        info!("card set: {:?}", card);
        self.card.clone().set(card.map(Arc::unwrap_or_clone));
        self.pos.clone().set(self.current_pos());
        self.show_backside.set(false);
    }
}
