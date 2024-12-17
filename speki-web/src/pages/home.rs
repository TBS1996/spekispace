use dioxus::prelude::*;
use tracing::info;

#[component]
pub fn Home() -> Element {
    info!("home nice");

    rsx! {}
}
