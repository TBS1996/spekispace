use std::sync::Arc;

use dioxus::prelude::*;

use crate::pop_overlay;

#[component]
pub fn TextInputRender(
    question: Arc<String>,
    input_value: Signal<String>,
    on_submit: EventHandler<String>,
) -> Element {
    rsx! {
        div {
            class: "flex flex-col items-center justify-center space-y-4 p-6 bg-gray-100 rounded-lg shadow-md",
            p {
                class: "text-lg font-semibold text-gray-800",
                "{question}"
            }

            input {
                class: "border border-gray-300 rounded px-4 py-2 focus:outline-none focus:ring-2 focus:ring-blue-300",
                value: "{input_value}",
                oninput: move |e| input_value.clone().set(e.value().clone()),
            }

            button {
                class: "{crate::styles::CREATE_BUTTON}",
                onclick: move |_| {
                    let value = input_value.cloned();
                    on_submit(value);
                    pop_overlay();
                },
                "Submit"
            }
        }
    }
}
