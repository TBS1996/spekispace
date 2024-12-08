use dioxus::prelude::*;
use tracing::info;

use crate::components::backside::BackPut;
use crate::components::card_selector;

use crate::App;

#[component]
pub fn Add() -> Element {
    let app = use_context::<App>();
    let mut frontside = use_signal(|| "".to_string());
    let backside = use_context::<BackPut>();

    let refsearch = backside.searching_cards.clone();

    rsx! {
        crate::nav::nav{}


        match refsearch() {
            Some(props) => rsx!{
                card_selector::card_selector {
                    title: props.title.clone(),
                    search: props.search.clone(),
                    on_card_selected: props.on_card_selected.clone(),
                    cards: props.cards.clone(),
                }
            },
            None => {
                rsx ! {
                    div {
                        style: "max-width: 500px; margin: 0 auto;",
                        div {
                            h1 {
                                class: "text-2xl font-bold text-gray-800 mb-6 text-center",
                                "Add Flashcard"
                            }

                            label {
                                class: "block text-gray-700 text-sm font-medium mb-2",
                                "Front:"
                                input {
                                    class: "w-full border border-gray-300 rounded-md p-2 mb-4 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                                    value: "{frontside}",
                                    oninput: move |evt| frontside.set(evt.value()),
                                }
                            }

                        { backside.view() }

                            button {
                                class: "bg-blue-500 text-white py-2 px-4 rounded-md hover:bg-blue-600 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 mt-4",
                                onclick: move |_| {
                                    let _app = app.clone();
                                    let _backside = backside.clone();
                                    spawn(async move {

                                        let front = format!("{frontside}");
                                        let Some(back) = _backside.to_backside() else {
                                            info!("oops, empty backside");
                                            return;
                                        };

                                        frontside.set(String::new());
                                        _backside.reset();

                                        info!("adding new card!");
                                        _app.0.add_card_with_backside(front, back).await;
                                    });
                                },
                                "Save"
                            }
                        }
                    }
                }
            }
        }
    }
}
