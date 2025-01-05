use std::sync::Arc;

use dioxus::prelude::*;

use super::Overlay;
use crate::components::Komponent;

#[derive(Clone)]
pub struct Yesno {
    question: Arc<String>,
    done: Signal<bool>,
    on_yes: Arc<Box<dyn Fn()>>,
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

impl Komponent for Yesno {
    fn render(&self) -> Element {
        let question = self.question.clone();
        let mut done = self.done.clone();
        let on_yes = self.on_yes.clone();

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
                        on_yes();
                        done.set(true);
                    },
                    "Yes"
                }

                button {
                    class: "bg-red-500 text-white px-4 py-2 rounded hover:bg-red-600 focus:outline-none focus:ring-2 focus:ring-red-300",
                    onclick: move |_| {
                        done.set(true);
                    },
                    "No"
                }
            }
        }
    }
}

impl Overlay for Yesno {
    fn is_done(&self) -> Signal<bool> {
        self.done.clone()
    }
}
