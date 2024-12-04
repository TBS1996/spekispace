use std::sync::Arc;

use dioxus::prelude::*;
use speki_core::{AnyType, Card};
use speki_web::BrowsePage;
use tracing::info;

use crate::{graph, pages::BrowseState, utils::App};

#[component]
pub fn display_card() -> Element {
    info!("rendering display_card");
    let browse_state = use_context::<BrowseState>();
    let app = use_context::<App>();
    let mut card = browse_state.selected_card.cloned().get_card().unwrap();
    let cyto_id = format!("cyto-{}", card.id.into_inner().to_string());

    let mut selected_card = browse_state.selected_card.clone();

    info!("huh??");
    if let Some(browse) = speki_web::take_browsepage() {
        info!("set some!!");
        selected_card.set(browse);
        let _app = app.clone();
        let _card = card.clone();
        let _id = cyto_id.clone();
        spawn(async move {
            graph::cyto_graph(&_id, _app, _card.clone()).await;
        });
    } else {
        info!("nope no set");
    }

    info!("cyto id: {cyto_id}");

    let browse_state = use_context::<BrowseState>();

    let mut front_input = browse_state.front_input.clone();
    let mut back_input = browse_state.back_input.clone();

    let _card = card.clone();
    let _app = app.clone();
    let _cyto_id = cyto_id.clone();
    let _selected = browse_state.selected_card.clone();
    use_effect(move || {
        let card = _card.clone();
        let app = _app.clone();
        let cyto_id = _cyto_id.clone();
        let _selected = _selected.clone();
        info!("GRAPH SHIT LOL");
        spawn(async move {
            let raw = card.to_raw();
            let front = raw.data.front.unwrap_or_default();
            let back = raw.data.back.unwrap_or_default().to_string();
            front_input.set(front);
            back_input.set(back);
        });
    });

    let _card = card.clone();
    let card2 = card.clone();
    let _app = app.clone();
    let id2 = cyto_id.clone();
    rsx! {
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
                        let id = id2.clone();
                        let app = _app.clone();
                        let card = card2.clone();
                        spawn(async move{
                            graph::cyto_graph(&id, app, card.clone()).await;
                        });
                    },
                    "re-render"
                }
                button {
                    class: "mt-6 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base md:mt-0",
                    onclick: move |_| {
                        let card = _card.clone();
                        let browse_state = use_context::<BrowseState>();
                        browse_state.selected_card.clone().set(BrowsePage::SetDependency(card));
                    },
                    "set dependency"
                }
                button {
                    class: "mt-6 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base md:mt-0",
                    onclick: move |_| {
                        let browse_state = use_context::<BrowseState>();
                        browse_state.selected_card.clone().set(BrowsePage::Browse);
                    },
                    "go back"
                }
                button {
                    class: "mt-6 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base md:mt-0",
                    onclick: move |_| {
                        let value = card.clone();
                        spawn(async move {
                            let front = front_input.cloned();
                            let back = back_input.cloned();
                            let mut card = (*value).clone();
                            let mut raw = card.to_raw();
                            raw.data.front = Some(front);
                            raw.data.back = Some(back.into());

                            info!("raw stuff: {raw:?}");

                            card.update_with_raw(raw).await;

                            let mut browse_state = use_context::<BrowseState>();
                            browse_state.selected_card.set(BrowsePage::Browse);
                            browse_state.refresh_cards().await;

                            });
                    },
                    "save"
                }
            }
        div {
            id: "{cyto_id}",
            style: "width: 800px; height: 600px; border: 1px solid black;",
        }
    }
}
