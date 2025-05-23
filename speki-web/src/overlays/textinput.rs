use std::sync::Arc;

use dioxus::prelude::*;

#[derive(Props, Clone)]
pub struct TextInput {
    pub question: Arc<String>,
    pub input_value: Signal<String>,
    pub done: Signal<bool>,
    pub on_submit: Arc<Box<dyn Fn(String)>>,
}

impl PartialEq for TextInput {
    fn eq(&self, other: &Self) -> bool {
        self.question == other.question
            && self.input_value == other.input_value
            && self.done == other.done
    }
}

impl TextInput {
    pub fn new(q: String, hook: Arc<Box<dyn Fn(String)>>, done: Signal<bool>) -> Self {
        Self {
            question: Arc::new(q),
            done,
            input_value: Signal::new_in_scope(Default::default(), ScopeId::APP),
            on_submit: hook,
        }
    }
}

#[component]
pub fn TextInputRender(props: TextInput) -> Element {
    let question = props.question.clone();
    let on_submit = props.on_submit.clone();
    let input_value = props.input_value.clone();

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
                },
                "Submit"
            }
        }
    }
}
