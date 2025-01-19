use std::{
    fmt::{Debug, Display},
    sync::Arc,
};

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use tracing::info;

use super::Komponent;
use crate::CURRENT_ROUTE;

impl<T> PartialEq for DropDownMenu<T>
where
    T: Serialize + for<'de> Deserialize<'de> + 'static + Clone + Display + PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.options == other.options && self.selected == other.selected && self.init == other.init
    }
}

#[derive(Clone)]
pub struct DropDownMenu<T>
where
    T: Serialize + for<'de> Deserialize<'de> + 'static + Clone + Display,
{
    pub options: Vec<T>,
    pub selected: Signal<T>,
    pub hook: Option<Arc<Box<dyn Fn(T)>>>,
    init: Signal<bool>,
}

impl<T: Serialize + for<'de> Deserialize<'de> + 'static + Clone + Display> Debug
    for DropDownMenu<T>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DropDownMenu").finish()
    }
}

impl<T> DropDownMenu<T>
where
    T: Serialize + for<'de> Deserialize<'de> + 'static + Clone + Display,
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

    pub fn with_hook(mut self, hook: Arc<Box<dyn Fn(T)>>) -> Self {
        self.hook = Some(hook);
        self
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
        let selv = self.clone();

        use_hook(|| {
            let _ = CURRENT_ROUTE.cloned();
            selv.set(dropdown.cloned());
            selv.init.clone().set(true);
        });

        // hack: without this it then at first it renders the first value in the options regardless of the default value set.
        if !selv.init.cloned() {
            selv.set(dropdown.cloned());
            selv.init.clone().set(true);
        }

        rsx! {
            div {
                class: "dropdown",
                select {
                    class: "appearance-none bg-white w-full border border-gray-300 rounded-md p-2 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                    style: "background-image: none;",
                    value: serde_json::to_string(&dropdown.cloned()).unwrap(),
                    onchange: move |evt| {
                        let new_choice: T =  serde_json::from_str(evt.value().as_str()).unwrap();
                        if let Some(hook) = selv.hook.as_ref() {
                            (hook)(new_choice.clone());
                        }
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
