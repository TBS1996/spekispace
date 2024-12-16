use std::{rc::Rc, sync::Arc};

use dioxus::prelude::*;
use speki_core::{AnyType, Card};
use speki_web::BrowsePage;
use tracing::{info, trace};

use crate::{
    overlays::card_selector::CardSelectorProps, pages::BrowseState, utils::App, OverlayManager,
    Popup,
};

#[component]
pub fn display_card() -> Element {
    info!("rendering display_card");
    let browse_state = use_context::<BrowseState>();
    let app = use_context::<App>();
    let card = browse_state.selected_card.cloned().get_card().unwrap();
    let cyto_id = format!("cyto-{}", card.id.into_inner().to_string());

    let mut selected_card = browse_state.selected_card.clone();

    let graphing = browse_state.graph.clone();
    if let Some(browse) = speki_web::take_browsepage() {
        info!("set some!!");
        selected_card.set(browse);
        let _app = app.clone();
        let _card = card.clone();
        let _id = cyto_id.clone();
        spawn(async move {
            graphing.read().set_card(_app, _card).await;
        });
    } else {
        trace!("nope no set");
    }

    info!("cyto id: {cyto_id}");

    let browse_state = use_context::<BrowseState>();

    let mut front_input = browse_state.front_input.clone();
    let mut back_input = browse_state.back_input.clone();

    let _card = card.clone();
    let _app = app.clone();
    let graph = browse_state.graph.clone();
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
                        let browse_state = use_context::<BrowseState>();
                        let selected_card = browse_state.selected_card.clone();
                        let fun = move |card: Arc<Card<AnyType>>| {
                            selected_card.clone().set(BrowsePage::View(card));

                        };

                        let props = CardSelectorProps {
                            title: "set dependency".to_string(),
                            search: browse_state.search.clone(),
                            on_card_selected: Rc::new(fun),
                            cards: browse_state.cards.clone(),
                            done: Default::default(),
                        };
                        let pop: Popup = Box::new(props);
                        use_context::<OverlayManager>().set(pop);



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
        { graph.read().render() }
    }
}
