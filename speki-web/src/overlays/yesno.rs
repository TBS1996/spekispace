use std::sync::Arc;

use dioxus::prelude::*;

use crate::components::Komponent;

use super::Overlay;

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
                // Render the question
                p {
                    class: "text-lg font-semibold text-gray-800",
                    "{question}"
                }

                // Render Yes button
                button {
                    class: "bg-green-500 text-white px-4 py-2 rounded hover:bg-green-600 focus:outline-none focus:ring-2 focus:ring-green-300",
                    onclick: move |_| {
                        on_yes(); // Call the on_yes callback
                        done.set(true); // Mark the component as done
                    },
                    "Yes"
                }

                // Render No button
                button {
                    class: "bg-red-500 text-white px-4 py-2 rounded hover:bg-red-600 focus:outline-none focus:ring-2 focus:ring-red-300",
                    onclick: move |_| {
                        done.set(true); // Mark the component as done
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
