use std::sync::Arc;

use dioxus::prelude::*;

use crate::{pop_overlay, styles::CRUD};

#[derive(Props, Clone)]
pub struct TextInput {
    pub question: Arc<String>,
    pub input_value: Signal<String>,
    pub on_submit: Arc<Box<dyn Fn(String)>>,
    pub crud: CRUD,
}

impl PartialEq for TextInput {
    fn eq(&self, other: &Self) -> bool {
        self.question == other.question && self.input_value == other.input_value
    }
}

impl TextInput {
    pub fn new(q: String, hook: Arc<Box<dyn Fn(String)>>) -> Self {
        Self {
            question: Arc::new(q),
            input_value: Signal::new_in_scope(Default::default(), ScopeId::APP),
            on_submit: hook,
            crud: CRUD::Create,
        }
    }

    pub fn with_crud(self, crud: CRUD) -> Self {
        Self { crud, ..self }
    }
}

#[component]
pub fn TextInputRender(props: TextInput) -> Element {
    let TextInput {
        question,
        input_value,
        on_submit,
        crud,
    } = props;

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
                class: "{crud.style()}",
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
