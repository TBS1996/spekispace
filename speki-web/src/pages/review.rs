use std::time::Duration;

use speki_dto::{Recall, Review as ReviewDTO};
use tracing::info;

use crate::{js, ReviewState, Route, State, REPO_PATH};
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

#[component]
pub fn Review() -> Element {
    let review = use_context::<ReviewState>();
    let card = review.card.clone();
    let pos = review.pos.clone();
    let tot = review.tot_len.clone();
    let mut show_backside = use_signal(|| false);

    let front = review.front.clone();
    let back = review.back.clone();
    //    let cloned_show = show_backside.clone();
    /*
    use_effect(move || {
        use wasm_bindgen::{prelude::Closure, JsCast};
        let mut show_backside = cloned_show;

        let callback = Closure::wrap(Box::new(move |e: web_sys::KeyboardEvent| {
            match e.key().as_str() {
                " " => {
                    spawn(async move {
                        show_backside.set(true);
                    });
                }
                "1" => {
                    spawn(async move {
                        let state = use_context::<State>();
                        let review = use_context::<ReviewState>();
                        review
                            .do_review(&state.app, new_review(Recall::Late), REPO_PATH)
                            .await;
                    });
                }
                "2" => {
                    spawn(async move {
                        if !*show_backside.read() {
                            return;
                        }
                        let state = use_context::<State>();
                        let review = use_context::<ReviewState>();
                        review
                            .do_review(&state.app, new_review(Recall::Late), REPO_PATH)
                            .await;
                        show_backside.set(false);
                    });
                }
                "3" => {
                    spawn(async move {
                        if !*show_backside.read() {
                            return;
                        }
                        let state = use_context::<State>();
                        let review = use_context::<ReviewState>();
                        review
                            .do_review(&state.app, new_review(Recall::Some), REPO_PATH)
                            .await;
                        show_backside.set(false);
                    });
                }
                "4" => {
                    spawn(async move {
                        if !*show_backside.read() {
                            return;
                        }

                        let state = use_context::<State>();
                        let review = use_context::<ReviewState>();
                        review
                            .do_review(&state.app, new_review(Recall::Perfect), REPO_PATH)
                            .await;
                        show_backside.set(false);
                    });
                }
                _ => {}
            }

            info!("Key pressed: {}", e.key());
        }) as Box<dyn FnMut(_)>);

        web_sys::window()
            .unwrap()
            .add_event_listener_with_callback("keydown", callback.as_ref().unchecked_ref())
            .unwrap();

        callback.forget();
    });
    */

    rsx! {
        div {
            div {
                tabindex: 0, // Ensures the div can capture keyboard events
                onkeydown: move |event| {
                    info!("Key pressed: {}", event.key());
                },
                "Press any key and check the console log."
            }
            Link {to: Route::Home {  }, "back home"}
            match card() {
                Some(_) => rsx! {
                    div {
                        h2 { "Reviewing Card {pos} of {tot}" }
                        p { "Front: {front}" }
                        if show_backside() {
                            p { "Back: {back}" }
                            div {
                                button {
                                    onclick: move |_| {
                                        spawn(async move{
                                            let state = use_context::<State>();
                                            let review = use_context::<ReviewState>();
                                            review.do_review(&state.app, new_review(Recall::None), REPO_PATH).await;
                                            show_backside.set(false);
                                        });
                                    },
                                    "No recall"
                                }
                                button {
                                    onclick: move |_| {
                                        spawn(async move{
                                            let state = use_context::<State>();
                                            let review = use_context::<ReviewState>();
                                            review.do_review(&state.app, new_review(Recall::Late), REPO_PATH).await;
                                            show_backside.set(false);
                                        });
                                    },
                                    "Bad recall"
                                }
                                button {
                                    onclick: move |_| {
                                        spawn(async move{
                                            let state = use_context::<State>();
                                            let review = use_context::<ReviewState>();
                                            review.do_review(&state.app, new_review(Recall::Some), REPO_PATH).await;
                                            show_backside.set(false);
                                        });
                                    },
                                    "Good recall"
                                }
                                button {
                                    onclick: move |_| {
                                        spawn(async move{
                                            let state = use_context::<State>();
                                            let review = use_context::<ReviewState>();
                                            review.do_review(&state.app, new_review(Recall::Perfect), REPO_PATH).await;
                                            show_backside.set(false);
                                        });
                                    },
                                    "Perfect recall"
                                }
                            }
                        } else {
                            button {
                                onclick: move |_| show_backside.set(true),
                                "show backside"
                            }
                        }
                    }
                },

                // If there's no card, display the "Start Review" button
                None => rsx! {
                    div {
                        p { "No cards to review." }
                        button {
                            onclick: move |_| {
                                spawn(
                                    async move {
                                        let state = use_context::<State>();
                                        let review = use_context::<ReviewState>();
                                        review.refresh(&state.app, DEFAULT_FILTER.to_string()).await;
                                    }
                                );

                            },
                            "Start Review"
                        }
                    }
                },
            }
        }
    }
}
