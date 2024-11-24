use crate::Route;

use super::*;

const DEFAULT_FILTER: &'static str =
    "recall < 0.8 & finished == true & suspended == false & resolved == true & minrecrecall > 0.8 & minrecstab > 10 & lastreview > 0.5 & weeklapses < 3 & monthlapses < 6";

pub fn new_review(recall: Recall) -> Review {
    Review {
        timestamp: js::current_time(),
        grade: recall,
        time_spent: Duration::default(),
    }
}

#[component]
pub fn Review() -> Element {
    let state = use_context::<State>();
    let review = use_context::<ReviewState>();
    let card = review.card.clone();
    let pos = review.pos.clone();
    let tot = review.tot_len.clone();
    let mut show_backside = use_signal(|| false);
    let path = "/foobar";

    let front = review.front.clone();
    let back = review.back.clone();

    rsx! {
        div {
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
                                        let review = review.clone();
                                        let state = state.clone();
                                        spawn(async move{
                                            review.do_review(&state.app, new_review(Recall::None), path).await;
                                        });
                                    },
                                    "No recall"
                                }
                                button {
                                    onclick: move |_| {
                                        spawn(async move{
                                            let state = use_context::<State>();
                                            let review = use_context::<ReviewState>();
                                            review.do_review(&state.app, new_review(Recall::Late), path).await;
                                        });
                                    },
                                    "Bad recall"
                                }
                                button {
                                    onclick: move |_| {
                                        spawn(async move{
                                            let state = use_context::<State>();
                                            let review = use_context::<ReviewState>();
                                            review.do_review(&state.app, new_review(Recall::Some),path).await;
                                        });
                                    },
                                    "Good recall"
                                }
                                button {
                                    onclick: move |_| {
                                        spawn(async move{
                                            let state = use_context::<State>();
                                            let review = use_context::<ReviewState>();
                                            review.do_review(&state.app, new_review(Recall::Perfect), path).await;
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
