use std::fmt::Display;

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use super::Komponent;

#[derive(Clone)]
pub struct DropDownMenu<T>
where
    T: Serialize + for<'de> Deserialize<'de> + 'static + Clone + Display,
{
    pub options: Vec<T>,
    pub selected: Signal<T>,
}

impl<T> DropDownMenu<T>
where
    T: Serialize + for<'de> Deserialize<'de> + 'static + Clone + Display,
{
    pub fn new<I>(options: I) -> Self
    where
        I: IntoIterator<Item = T>,
    {
        let options: Vec<T> = options.into_iter().collect();
        assert!(!options.is_empty(), "must provide at least one option");
        let selected = Signal::new(options.first().unwrap().clone());

        Self { options, selected }
    }

    pub fn reset(&self) {
        let first = self.options.first().unwrap().clone();
        self.selected.clone().set(first);
    }
}

impl<T> Komponent for DropDownMenu<T>
where
    T: Serialize + for<'de> Deserialize<'de> + 'static + Clone + Display,
{
    fn render(&self) -> Element {
        let mut dropdown = self.selected.clone();
        let val: String = serde_json::to_string(&dropdown.cloned()).unwrap();

        rsx! {
            div {
                class: "dropdown",
                select {
                    class: "w-full border border-gray-300 rounded-md p-2 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                    value: "{val}",
                    onchange: move |evt| {
                        let new_choice: T =  serde_json::from_str(evt.value().as_str()).unwrap();
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
