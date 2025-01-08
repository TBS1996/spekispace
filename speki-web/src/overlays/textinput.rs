use std::sync::Arc;

use dioxus::prelude::*;

use super::Overlay;
use crate::components::Komponent;

#[derive(Clone)]
pub struct TextInput {
    question: Arc<String>,
    input_value: Signal<String>,
    done: Signal<bool>,
    on_submit: Arc<Box<dyn Fn(String)>>,
}

impl TextInput {
    pub fn new(q: String, hook: Arc<Box<dyn Fn(String)>>) -> Self {
        Self {
            question: Arc::new(q),
            done: Signal::new_in_scope(false, ScopeId::APP),
            input_value: Signal::new_in_scope(Default::default(), ScopeId::APP),
            on_submit: hook,
        }
    }
}

impl Komponent for TextInput {
    fn render(&self) -> Element {
        let question = self.question.clone();
        let mut done = self.done.clone();
        let on_submit = self.on_submit.clone();

        let input_value = self.input_value.clone();

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
                    class: "bg-blue-500 text-white px-4 py-2 rounded hover:bg-blue-600 focus:outline-none focus:ring-2 focus:ring-blue-300",
                    onclick: move |_| {
                        let value = input_value.cloned();
                        on_submit(value);
                        done.set(true);
                    },
                    "Submit"
                }
            }
        }
    }
}

impl Overlay for TextInput {
    fn is_done(&self) -> Signal<bool> {
        self.done.clone()
    }
}
