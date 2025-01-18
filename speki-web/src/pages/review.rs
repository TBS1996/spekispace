use std::sync::Arc;

use dioxus::prelude::*;
use speki_core::{cardfilter::CardFilter, collection::Collection};

use crate::{
    components::{FilterEditor, Komponent},
    overlays::{colviewer::ColViewer, reviewsession::ReviewState, textinput::TextInput},
    APP, IS_SHORT, OVERLAY,
};

#[derive(Clone)]
pub struct ReviewPage {
    filter: FilterEditor,
    collections: Signal<Vec<Collection>>,
}

impl ReviewPage {
    pub fn new() -> Self {
        let selv = Self {
            filter: FilterEditor::new_default(),
            collections: Default::default(),
        };

        let cols = selv.collections.clone();

        spawn(async move {
            let _cols = APP.read().load_collections().await;
            cols.clone().set(_cols);
        });

        selv
    }
}

#[component]
pub fn Review() -> Element {
    let state: ReviewPage = use_context::<ReviewPage>();
    let editor = state.filter.clone();

    let class = if IS_SHORT.cloned() {
        "flex flex-col items-center h-screen space-y-4 justify-center"
    } else {
        "flex flex-col items-start h-screen space-y-4 pl-32"
    };

    rsx! {
        div {
            class: "{class}",

            div {
                class: "flex space-x-4 mt-6",

                { render_collections(state) }


                {editor.render()}

            }
        }
    }
}

fn render_collections(state: ReviewPage) -> Element {
    let filter = state.filter.to_filter();
    let collections = state.collections.clone();

    let mut colfil: Vec<(Collection, CardFilter)> = vec![];

    for col in collections.cloned() {
        colfil.push((col, filter.clone()));
    }

    rsx! {
        div {
            class: "flex flex-col max-w-[350px] mr-5",

            button {
                class: "inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base mb-2",
                onclick: move |_| {
                    let filter = filter.clone();
                    spawn(async move {
                        let cards = APP.read().load_all(Some(filter)).await;
                        let session = ReviewState::new(cards).await;
                        OVERLAY.cloned().set(Box::new(session));
                    });
                },
                "review all"

            }

            for (col, filter) in colfil {
                div {
                    class: "flex flex-row",
                    button {
                        class: "inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base mb-2",
                        onclick: move |_| {
                            let filter = filter.clone();
                            spawn(async move {
                                let col = APP.read().load_collection(col.id).await;
                                let cards = col.expand(APP.read().inner().card_provider.clone()).await;
                                let session = ReviewState::new_with_filter(cards, filter).await;
                                OVERLAY.cloned().set(Box::new(session));
                            });
                        },
                        "{col.name}"
                    }
                    button {
                        onclick: move |_|{
                            spawn(async move {
                                let viewer = ColViewer::new(col.id).await;
                                OVERLAY.read().set(Box::new(viewer));
                            });
                        },
                        "✏️"
                    }
                    button {
                        onclick: move|_| {
                            let id = col.id;
                            let mut cols = collections.cloned();
                            cols.retain(|kol|kol.id != col.id);
                            collections.clone().set(cols);
                            spawn(async move{
                                APP.read().delete_collection(id).await;
                            });
                        },
                        "❌"
                    }
                }
            }

            button {
                class: "inline-flex items-center text-white bg-blue-700 border-0 py-1 px-3 focus:outline-none hover:bg-blue-900 rounded text-base mb-5",
                onclick: move |_| {
                    let f = move |name: String| {
                        let col = Collection::new(name);
                        spawn(async move {
                            APP.read().save_collection(col).await;
                        });
                    };

                    let txt = TextInput::new("add collection".to_string(), Arc::new(Box::new(f)));
                    OVERLAY.read().set(Box::new(txt));
                },
                "add collection"
            }


        }
    }
}
