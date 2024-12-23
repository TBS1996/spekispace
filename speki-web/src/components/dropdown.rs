use std::fmt::Display;

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use tracing::info;

use super::Komponent;

#[derive(Clone)]
pub struct DropDownMenu<T>
where
    T: Serialize + for<'de> Deserialize<'de> + 'static + Clone + Display,
{
    pub options: Vec<T>,
    pub selected: Signal<T>,
    id: ScopeId,
}

impl<T> DropDownMenu<T>
where
    T: Serialize + for<'de> Deserialize<'de> + 'static + Clone + Display,
{
    pub fn new(options: impl IntoIterator<Item = T>) -> Self {
        let options: Vec<T> = options.into_iter().collect();
        assert!(!options.is_empty(), "must provide at least one option");
        let selected = Signal::new_in_scope(options.first().unwrap().clone(), ScopeId(3));
        let id = current_scope_id().unwrap();

        Self {
            options,
            selected,
            id,
        }
    }

    pub fn reset(&self) {
        let first = self.options.first().unwrap().clone();
        self.selected.clone().set(first);
    }

    pub fn with_id(mut self, id: ScopeId) -> Self {
        self.id = id;
        self
    }
}

impl<T> Komponent for DropDownMenu<T>
where
    T: Serialize + for<'de> Deserialize<'de> + 'static + Clone + Display,
{
    fn render(&self) -> Element {
        let mut dropdown = self.selected.clone();
        let selv = self.clone();
        let val: String = serde_json::to_string(&dropdown.cloned()).unwrap();

        rsx! {
            div {
                class: "dropdown",
                select {
                    class: "bg-white w-full border border-gray-300 rounded-md p-2 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                    value: "{val}",
                    onchange: move |evt| {
                        let new_choice: T =  serde_json::from_str(evt.value().as_str()).unwrap();
                        selv.id.needs_update();
                        dropdown.set(new_choice);
                    },

                    for opt in &self.options {
                        option { value: serde_json::to_string(&opt).unwrap(), "{opt}" }
                    }
                }
            }
        }
    }
}
