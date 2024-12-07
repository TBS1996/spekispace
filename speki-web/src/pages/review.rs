use std::rc::Rc;

use dioxus::prelude::*;
use speki_dto::Recall;
use tracing::info;

use crate::review_state::ReviewState;

pub const DEFAULT_FILTER: &'static str =
    "recall < 0.8 & finished == true & suspended == false & minrecrecall > 0.8 & lastreview > 0.5 & weeklapses < 3 & monthlapses < 6";

fn recall_button(recall: Recall) -> Element {
    let label = match recall {
        Recall::None => "unfamiliar",
        Recall::Late => "recognized",
        Recall::Some => "recalled",
        Recall::Perfect => "mastered",
    };

    rsx! {
        button {
            class: "mt-6 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base md:mt-0",
            onclick: move |_| {
                spawn(async move{
                    let mut review = use_context::<ReviewState>();
                    review.do_review(recall).await;
                });
            },
            "{label}"

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
                        let mut review = use_context::<ReviewState>();
                        review.refresh(DEFAULT_FILTER.to_string()).await;
                    });
                },
                "Start review"
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

#[component]
pub fn Review() -> Element {
    let review = use_context::<ReviewState>();
    let card = review.card.clone();
    let pos = review.pos.clone();
    let tot = review.tot_len.clone();
    let reviewing = card().is_some();
    let mut show_backside = review.show_backside.clone();

    let front = review.front.clone();
    let back = review.back.clone();

    let log_event = move |event: Rc<KeyboardData>| {
        let _review = review.clone();
        spawn(async move {
            info!("reviewing..");
            let mut rev = _review.clone();
            match event.key().to_string().as_str() {
                "1" => rev.do_review(Recall::None).await,
                "2" => rev.do_review(Recall::Late).await,
                "3" => rev.do_review(Recall::Some).await,
                "4" => rev.do_review(Recall::Perfect).await,
                " " => rev.show_backside.set(true),
                _ => {}
            }
        });
    };

    rsx! {

        crate::nav::nav{}

        div { id: "receiver", tabindex: 0,
            onkeydown: move |event| log_event(event.data()),

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
                        { review_buttons() }
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
