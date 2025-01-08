use dioxus::prelude::*;
use speki_core::collection::Collection;

use crate::APP;

#[component]
pub fn Collections() -> Element {
    let collections: Signal<Vec<Collection>> = use_signal(|| vec![]);
    spawn(async move {
        let cols = APP.read().load_collections().await;
        collections.clone().set(cols);
    });

    rsx! {
        for col in collections() {
            p {"{col.name}"}
        }
    }
}
