use dioxus::prelude::*;

#[derive(Clone)]
pub struct Notice {
    pub text: String,
}

impl Notice {
    pub fn new_from_debug<T: std::fmt::Debug>(elm: T) -> Self {
        let text = format!("{:?}", elm);

        Self { text }
    }
}

#[component]
pub fn NoticeRender(text: String) -> Element {
    rsx! {
        p {"{text}"}
    }
}
