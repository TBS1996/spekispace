use std::fmt::{Debug, Display};

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use tracing::info;

#[component]
pub fn DropComponent<T: PartialEq + Clone + 'static>(
    options: Vec<T>,
    selected: Signal<T>,
    hook: Option<Callback<T, ()>>,
) -> Element
where
    T: Serialize + for<'de> Deserialize<'de> + 'static + Clone + Display,
{
    let mut dropdown = selected.clone();
    let value = serde_json::to_string(&dropdown.cloned()).unwrap();
    info!("value: {value}");

    rsx! {
        div {
            class: "dropdown",
            select {
                class: "appearance-none bg-white w-full border border-gray-300 rounded-md p-2 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                style: "background-image: none;",
                value: "{value}",
                onchange: move |evt| {
                    let new_choice: T =  serde_json::from_str(evt.value().as_str()).unwrap();
                    if let Some(hook) = hook{
                        (hook)(new_choice.clone());
                    }
                    dropdown.set(new_choice);
                },

                for opt in options {
                    option {
                        value: serde_json::to_string(&opt).unwrap(),
                        selected: *selected.read() == opt,
                        "{opt}"
                    }
                }
            }
        }
    }
}

impl<T> PartialEq for DropDownMenu<T>
where
    T: Serialize + for<'de> Deserialize<'de> + 'static + Clone + Display + PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.options == other.options && self.selected == other.selected && self.init == other.init
    }
}

#[derive(Clone, Props)]
pub struct DropDownMenu<T>
where
    T: Serialize + for<'de> Deserialize<'de> + 'static + Clone + Display + PartialEq,
{
    pub options: Vec<T>,
    pub selected: Signal<T>,
    #[props(!optional)]
    pub hook: Option<Callback<T, ()>>,
    pub init: Signal<bool>,
}

impl<T: Serialize + for<'de> Deserialize<'de> + 'static + Clone + Display + PartialEq> Debug
    for DropDownMenu<T>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DropDownMenu").finish()
    }
}

impl<T> DropDownMenu<T>
where
    T: Serialize + for<'de> Deserialize<'de> + 'static + Clone + Display + PartialEq,
{
    pub fn new(options: impl IntoIterator<Item = T>, default: Option<T>) -> Self {
        info!("creating dopdown");
        let options: Vec<T> = options.into_iter().collect();
        assert!(!options.is_empty(), "must provide at least one option");

        let selected = match default {
            Some(x) => x,
            None => options.iter().next().cloned().unwrap(),
        };

        info!("selected val is: {selected}");

        let selected = Signal::new_in_scope(selected, ScopeId(3));

        Self {
            options,
            selected,
            hook: None,
            init: Signal::new_in_scope(false, ScopeId::APP),
        }
    }

    pub fn set(&self, choice: T) {
        self.selected.clone().set(choice);
    }

    pub fn with_callback(mut self, callback: Callback<T, ()>) -> Self {
        self.hook = Some(callback);
        self
    }

    pub fn reset(&self) {
        let first = self.options.first().unwrap().clone();
        self.selected.clone().set(first);
    }
}
