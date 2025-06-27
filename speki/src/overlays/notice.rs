use dioxus::prelude::*;

use crate::pop_overlay;

#[derive(Clone, PartialEq, Eq)]
pub struct Notice {
    pub text: String,
    pub button_text: String,
}

impl Notice {
    pub fn new_from_debug<T: std::fmt::Debug>(elm: T) -> Self {
        let text = format!("{:?}", elm);
        Self::new(text)
    }

    pub fn new(text: impl AsRef<str>) -> Self {
        let text = text.as_ref().to_string();
        Self {
            text,
            button_text: "OK".to_string(),
        }
    }
}

#[component]
pub fn NoticeRender(notice: Notice) -> Element {
    let Notice { text, button_text } = notice;
    rsx! {
        div {
            class: "fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50",
            div {
                class: "bg-white text-black rounded-2xl shadow-xl p-6 max-w-md text-center",
                h2 {
                    class: "text-xl font-semibold mb-4",
                    "{text}"
                }
                button {
                    class: "bg-blue-500 hover:bg-blue-600 text-white font-semibold py-2 px-4 rounded-lg",
                    onclick: move |_| {
                        pop_overlay();
                    },
                    "{button_text}"
                }
            }
        }
    }
}
