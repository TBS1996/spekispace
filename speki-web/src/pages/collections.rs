use std::sync::Arc;

use dioxus::prelude::*;
use speki_core::collection::Collection;

use crate::{
    overlays::{colviewer::ColViewer, textinput::TextInput},
    APP, OVERLAY,
};

#[component]
pub fn Collections() -> Element {
    let collections: Signal<Vec<Collection>> = use_signal(|| vec![]);
    spawn(async move {
        let cols = APP.read().load_collections().await;
        if cols.len() != collections.read().len() {
            collections.clone().set(cols);
        }
    });

    rsx! {
        div {
            class: "flex flex-col max-w-[350px] ml-5",

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
            for col in collections() {
                button {
                    class: "inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base mb-2",
                    onclick: move |_| {
                        spawn(async move {
                            let viewer = ColViewer::new(col.id).await;
                            OVERLAY.read().set(Box::new(viewer));
                        });
                    },
                    "{col.name}"
                }
            }
        }
    }
}
