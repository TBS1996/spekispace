use std::sync::Arc;

use dioxus::prelude::*;
use tracing::info;

#[derive(Props, Clone)]
pub struct Yesno {
    pub question: Arc<String>,
    pub done: Signal<bool>,
    pub on_yes: Arc<Box<dyn Fn()>>,
}

impl PartialEq for Yesno {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Yesno {
    pub fn new(q: String, hook: Arc<Box<dyn Fn()>>) -> Self {
        Self {
            question: Arc::new(q),
            done: Signal::new_in_scope(false, ScopeId::APP),
            on_yes: hook,
        }
    }
}

#[component]
pub fn YesnoRender(props: Yesno) -> Element {
    let question = props.question.clone();
    let mut done = props.done.clone();
    let on_yes = props.on_yes.clone();

    rsx! {
        div {
            class: "flex flex-col items-center justify-center space-y-4 p-6 bg-gray-100 rounded-lg shadow-md",
            p {
                class: "text-lg font-semibold text-gray-800",
                "{question}"
            }

            button {
                class: "bg-green-500 text-white px-4 py-2 rounded hover:bg-green-600 focus:outline-none focus:ring-2 focus:ring-green-300",
                onclick: move |_| {
                    info!("yes!");
                    on_yes();
                    done.set(true);
                },
                "Yes"
            }

            button {
                class: "bg-red-500 text-white px-4 py-2 rounded hover:bg-red-600 focus:outline-none focus:ring-2 focus:ring-red-300",
                onclick: move |_| {
                    info!("no!");
                    done.set(true);
                },
                "No"
            }
        }
    }
}
