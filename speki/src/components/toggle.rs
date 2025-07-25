use crate::components::dropdown::DropComponent;
use dioxus::prelude::*;

#[component]
pub fn Toggle(text: &'static str, b: Signal<bool>) -> Element {
    rsx! {
        div {
            class: "flex flex-row items-center mb-4",
            div {
                class: "w-24",
                p {
                    title: "card has room for improvement",
                    "{text}"
                }
            }
            DropComponent { options: vec![false, true], selected: b }
        }
    }
}
