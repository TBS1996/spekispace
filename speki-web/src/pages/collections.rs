use std::sync::Arc;

use dioxus::prelude::*;
use speki_core::collection::Collection;

use crate::{overlays::textinput::TextInput, APP, OVERLAY};

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
        button {
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
            p {"{col.name}"}
        }
    }
}
