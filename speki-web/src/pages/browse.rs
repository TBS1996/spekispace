use std::sync::Arc;

use dioxus::prelude::*;
use speki_core::{AnyType, Card};
use tracing::info;

use crate::App;

#[derive(Clone)]
struct CardEntry {
    front: String,
    card: Arc<Card<AnyType>>,
}

#[component]
pub fn Browse() -> Element {
    let cards: Signal<Vec<CardEntry>> = use_signal(|| vec![]);
    let mut search = use_signal(String::default);

    let mut selected_card: Signal<Option<Arc<Card<AnyType>>>> = use_signal(|| None);

    use_effect(move || {
        let mut cards = cards.clone();
        spawn(async move {
            let app = use_context::<App>();
            let mut out = vec![];
            for card in app.as_ref().load_all_cards().await {
                out.push(CardEntry {
                    front: card.print().await,
                    card,
                });
            }

            cards.set(out);
        });
    });

    let pat = search.cloned();
    let inner = cards
        .cloned()
        .into_iter()
        .filter(|card| card.front.contains(&pat))
        .take(50);

    let flag = selected_card.as_ref().is_some();

    let mut front_input = use_signal(String::default);
    let mut back_input = use_signal(String::default);

    rsx! {

        crate::nav::nav {}

        if flag {
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
                        selected_card.set(None);
                    },
                    "go back"
                }
                button {
                    class: "mt-6 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base md:mt-0",
                    onclick: move |_| {
                        let mut cards = cards.clone();
                        spawn(async move {

                        let front = front_input.cloned();
                        let back = back_input.cloned();
                        let card = selected_card.cloned().unwrap();
                        let mut raw = card.to_raw();
                        raw.data.front = Some(front);
                        raw.data.back = Some(back.into());

                        info!("raw stuff: {raw:?}");

                        (*card).clone().update_with_raw(raw).await;

                        let app = use_context::<App>();
                        let mut out = vec![];
                        for card in app.as_ref().load_all_cards().await {
                            out.push(CardEntry {
                                front: card.print().await,
                                card,
                            });
                        }

                        cards.set(out);

                        selected_card.set(None);


                        });
                    },
                    "save"
                }
            }

        } else {
            input {
                class: "w-full border border-gray-300 rounded-md p-2 mb-4 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                value: "{search}",
                oninput: move |evt| search.set(evt.value()),
            }

            div {
                style: "display: flex; flex-direction: column; gap: 8px; text-align: left;",

                for card in inner {
                    button {
                        style: "text-align: left;",
                        onclick: move |_| {
                            selected_card.set(Some(card.card.clone()));
                            info!("selected: {selected_card:?}");
                            let raw = card.card.to_raw();
                            let front = raw.data.front.unwrap_or_default();
                            let back = raw.data.back.map(|back|back.to_string()).unwrap_or_default();
                            front_input.set(front);
                            back_input.set(back);
                        },
                        "{card.front}"
                    }
                }
            }
        }
    }
}
