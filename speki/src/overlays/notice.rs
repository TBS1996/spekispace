use dioxus::prelude::*;

use crate::pop_overlay;

#[component]
pub fn NoticeRender(text: String, button_text: String) -> Element {
    rsx! {
        div {
            class: "fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50",
            div {
                class: "bg-white text-black rounded-2xl shadow-xl p-6 max-w-md text-center",
                h2 {
                    class: "text-xl font-semibold mb-4",
                    for line in text.lines() {
                        span { "{line}" }
                        br {}

                    }
                }

                button {
                    class: "{crate::styles::READ_BUTTON}",
                    onclick: move |_| {
                        pop_overlay();
                    },
                    "{button_text}"
                }
            }
        }
    }
}
