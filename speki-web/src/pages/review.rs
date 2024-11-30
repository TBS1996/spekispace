use std::time::Duration;

use speki_dto::{Recall, Review as ReviewDTO};

use crate::review_state::ReviewState;
use crate::{js, REPO_PATH};
use dioxus::prelude::*;

const DEFAULT_FILTER: &'static str =
    "recall < 0.8 & finished == true & suspended == false & minrecrecall > 0.8 & lastreview > 0.5 & weeklapses < 3 & monthlapses < 6";

pub fn new_review(recall: Recall) -> ReviewDTO {
    ReviewDTO {
        timestamp: js::current_time(),
        grade: recall,
        time_spent: Duration::default(),
    }
}

struct RecallButton {
    backside: Signal<bool>,
}

impl RecallButton {
    fn new(backside: Signal<bool>) -> Self {
        Self { backside }
    }

    fn create(&self, recall: Recall) -> Element {
        let mut show_backside = self.backside.clone();

        let label = match recall {
            Recall::None => "unfamiliar",
            Recall::Late => "recognized",
            Recall::Some => "recalled",
            Recall::Perfect => "mastered",
        };

        rsx! {
            button {
             //   class: "bg-gray-800 text-white rounded",
                //class: "bg-gray-800 text-white rounded-lg px-4 py-2 text-lg font-semibold shadow-md hover:bg-gray-700 focus:outline-none focus:ring-2 focus:ring-gray-500 transition-transform transform hover:scale-105",
                class: "mt-6 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base md:mt-0",
                onclick: move |_| {
                    spawn(async move{
                        let review = use_context::<ReviewState>();
                        review.do_review(new_review(recall), REPO_PATH).await;
                        show_backside.set(false);
                    });
                },
                "{label}"

            }
        }
    }
}

fn review_start() -> Element {
    rsx! {
        div {
            class: "flex items-center justify-center h-screen",
            button {
                class: "mt-6 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base md:mt-0",
                onclick: move |_| {
                    spawn(async move {
                        let review = use_context::<ReviewState>();
                        review.refresh(DEFAULT_FILTER.to_string()).await;
                    });
                },
                "Start review"
            }
        }
    }
}

fn review_buttons(recall_button_builder: RecallButton) -> Element {
    rsx! {
        div {
            style: "display: flex; gap: 16px; justify-content: center; align-items: center;",
            { recall_button_builder.create(Recall::None) }
            { recall_button_builder.create(Recall::Late) }
            { recall_button_builder.create(Recall::Some) }
            { recall_button_builder.create(Recall::Perfect) }
        }
    }
}

#[component]
pub fn Review() -> Element {
    let review = use_context::<ReviewState>();
    let card = review.card.clone();
    let pos = review.pos.clone();
    let tot = review.tot_len.clone();
    let reviewing = card().is_some();
    let mut show_backside = use_signal(|| false);

    let front = review.front.clone();
    let back = review.back.clone();

    let recall_button_builder = RecallButton::new(show_backside.clone());

    rsx! {

        { crate::nav::nav() }
        div {
            if reviewing {
                div {
                    class: "w-full max-w-lg text-center",

                    h2 {
                        class: "text-2xl text-gray-700 mb-6",
                        style: "width: 50%; margin: 0 auto; text-align: left;",
                        "{pos}/{tot}"
                    }
                    p {
                        class: "text-lg text-gray-800 mb-8",
                        "{front}"
                    }
                    if show_backside() {
                        div {
                            style: "width: 50%; border-top: 2px solid #d1d5db; margin: 24px auto;",
                        }
                        p {
                            class: "text-lg text-gray-700 mb-6",
                            style: "margin-bottom: 24px;",
                            "{back}"
                        }
                        { review_buttons(recall_button_builder) }
                    } else {
                        button {
                            class: "mt-6 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base md:mt-0",
                            onclick: move |_| show_backside.set(true),
                            "show backside"
                        }
                    }
                }
            } else {
                { review_start() }
            }
        }
    }
}
