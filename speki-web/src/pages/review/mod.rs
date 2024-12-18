use std::rc::Rc;

use dioxus::prelude::*;
use review_state::ReviewState;
use speki_dto::Recall;
use tracing::info;

use crate::Komponent;

pub mod review_state;

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

fn review_start(mut filter: Signal<String>) -> Element {
    rsx! {
        div {
            class: "flex items-center justify-center h-screen",

            input {
                class: "w-full border border-gray-300 rounded-md p-2 mb-4 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                value: "{filter}",
                oninput: move |evt| filter.set(evt.value().clone()),
            },

            button {
                class: "mt-6 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base md:mt-0",


                onclick: move |_| {
                    info!("review..?");
                    let mut review = use_context::<ReviewState>();
                    spawn(async move {
                        info!("nice..?");
                        review.refresh().await;
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
    let rev2 = review.clone();
    let card = review.card.clone();
    let pos = review.pos.clone();
    let tot = review.tot_len.clone();
    let _card = card.clone();
    let reviewing = use_memo(move || _card().is_some());
    let mut show_backside = review.show_backside.clone();
    let filter_sig = review.filter.clone();

    let front = review.front.clone();
    let back = review.back.clone();

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
            class: "flex justify-center items-center w-full h-screen p-4",
            tabindex: 0,
            onkeydown: move |event| log_event(event.data()),

            if reviewing() {
                div {
                    class: "w-full max-w-4xl flex flex-row gap-8 items-start",
                    div {
                        class: "flex-1 text-center flex flex-col gap-6",

                        div {
                            class: "flex justify-between items-center",
                            h2 {
                                class: "text-2xl text-gray-700",
                                "{pos}/{tot}"
                            }
                        }

                        p {
                            class: "text-lg text-gray-800",
                            "{front}"
                        }

                        if show_backside() {
                            div {
                                class: "w-full border-t-2 border-gray-300 my-6"
                            }
                            p {
                                class: "text-lg text-gray-700",
                                "{back}"
                            }

                            div {
                                class: "flex justify-center gap-4 mt-4",
                                { review_buttons() }
                            }
                        } else {
                            button {
                                class: "mt-6 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base",
                                onclick: move |_| show_backside.set(true),
                                "show backside"
                            }
                        }
                    }

                    if show_backside() {
                        div {
                            class: "flex-shrink-0 w-1/3",
                            { rev2.graph.render() }
                        }
                    }
                }
            } else {
                { review_start(filter_sig) }
            }
        }
    }
}
