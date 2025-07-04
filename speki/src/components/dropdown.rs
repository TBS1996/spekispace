use std::{
    fmt::{Debug, Display},
    sync::Arc,
};

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use tracing::trace;

pub type DropdownClosure = Arc<Box<dyn Fn()>>;

#[derive(Clone)]
pub struct DropdownAction {
    label: String,
    action: DropdownClosure,
    title: Option<String>,
}

impl DropdownAction {
    pub fn new(label: impl AsRef<str>, f: Box<dyn Fn()>) -> Self {
        Self {
            label: label.as_ref().to_string(),
            action: Arc::new(f),
            title: None,
        }
    }

    pub fn with_title(self, title: impl AsRef<str>) -> Self {
        Self {
            title: Some(title.as_ref().to_string()),
            ..self
        }
    }
}

impl PartialEq for DropdownAction {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

#[component]
pub fn ActionDropdown(
    label: String,
    options: Vec<DropdownAction>,
    #[props(default = false)] full_width: bool,
    #[props(default = "")] title: &'static str,
) -> Element {
    let mut current_value = use_signal(|| "".to_string());

    let wrapper_class = if full_width {
        "relative inline-block w-full"
    } else {
        "relative inline-block"
    };

    let select_class = "\
        appearance-none bg-white border border-gray-300 \
        text-gray-700 py-2 px-3 pr-8 rounded-md shadow-sm \
        focus:outline-none focus:ring-2 focus:ring-blue-500 \
        focus:border-blue-500 transition";

    rsx! {
        div {
            class: wrapper_class,

            select {
                class: if full_width {
                    format!("w-full {select_class}")
                } else {
                    select_class.to_string()
                },
                value: "{current_value}",
                title: title,
                onchange: move |evt| {
                    let val = evt.value();
                    if let Ok(idx) = val.parse::<usize>() {
                        if let Some(action) = options.get(idx) {
                            (action.action)();
                        }
                    }
                    current_value.set("".to_string());
                },

                option {
                    value: "",
                    selected: true,
                    disabled: true,
                    class: "text-gray-400",
                    "{label}"
                }

                for (i, action) in options.iter().enumerate() {
                    option {
                        value: i.to_string(),
                        title: "{action.title.clone().unwrap_or_default()}",
                        "{action.label}"
                    }
                }
            }

            div {
                class: "pointer-events-none absolute inset-y-0 right-0 flex items-center px-2 text-gray-400",
                svg {
                    xmlns: "http://www.w3.org/2000/svg",
                    class: "h-4 w-4",
                    fill: "none",
                    view_box: "0 0 24 24",
                    stroke: "currentColor",
                    stroke_width: "2",
                    path {
                        d: "M19 9l-7 7-7-7"
                    }
                }
            }
        }
    }
}

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
    tracing::trace!("value: {value}");

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
    pub display: Option<Arc<Box<dyn Fn(&T) -> String>>>,
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
        trace!("creating dopdown");
        let options: Vec<T> = options.into_iter().collect();
        assert!(!options.is_empty(), "must provide at least one option");

        let selected = match default {
            Some(x) => x,
            None => options.iter().next().cloned().unwrap(),
        };

        trace!("selected val is: {selected}");

        let selected = Signal::new_in_scope(selected, ScopeId(3));

        Self {
            options,
            selected,
            hook: None,
            init: Signal::new_in_scope(false, ScopeId::APP),
            display: None,
        }
    }

    pub fn with_display_fn(self, f: Arc<Box<dyn Fn(&T) -> String>>) -> Self {
        Self {
            display: Some(f),
            ..self
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
