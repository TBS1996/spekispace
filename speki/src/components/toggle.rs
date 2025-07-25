use dioxus::prelude::*;

#[component]
pub fn Toggle(text: &'static str, b: Signal<bool>) -> Element {
    rsx! {
        div {
            class: "flex items-center gap-4 mb-4",
            div {
                class: "w-24",
                p {
                    title: "card has room for improvement",
                    "{text}"
                }
            }
            div {
                class: "relative inline-block w-12 h-6 cursor-pointer",
                onclick: move |_| b.set(!b()),
                div {
                    class: "absolute top-0 left-0 w-full h-full rounded-full transition-colors duration-50 ease-in-out",
                    class: if b() {
                        "bg-blue-500"
                    } else {
                        "bg-gray-400"
                    }
                }
                div {
                    class: "absolute top-0.5 left-0.5 w-5 h-5 bg-white rounded-full shadow-md transition-transform duration-50 ease-in-out",
                    class: if b() {
                        "translate-x-6"
                    } else {
                        "translate-x-0"
                    }
                }
            }
        }
    }
}
