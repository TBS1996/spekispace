use std::{fmt::Display, sync::Arc};

use dioxus::prelude::*;

#[derive(Props, Clone)]
pub struct ItemSelector<T: Display + Clone + PartialEq + 'static> {
    pub items: Vec<T>,
    pub on_selected: Arc<Box<dyn Fn(T)>>,
    pub done: Signal<bool>,
}

impl<T: Display + Clone + PartialEq> PartialEq for ItemSelector<T> {
    fn eq(&self, other: &Self) -> bool {
        self.items == other.items && self.done == other.done
    }
}

impl<T: Display + Clone + PartialEq> ItemSelector<T> {
    pub fn new(items: Vec<T>, on_selected: Arc<Box<dyn Fn(T)>>) -> Self {
        Self {
            items,
            on_selected,
            done: Signal::new_in_scope(false, ScopeId::APP),
        }
    }
}

#[component]
pub fn ItemSelectorRender<T: Display + Clone + PartialEq>(props: ItemSelector<T>) -> Element {
    let mut iter: Vec<(T, Arc<Box<dyn Fn(T)>>, Signal<bool>)> = vec![];

    for item in props.items.clone() {
        iter.push((item, props.on_selected.clone(), props.done.clone()));
    }

    rsx! {
        div {
        class: "flex flex-col mb-10",
        for (item, on_select, mut is_done) in iter {
            button {
                class: "mt-2 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base md:mt-0",

                onclick: move |_| {
                    on_select(item.clone());
                    is_done.set(true);
                },
                "{item}"
            }
        }
    }
    }
}
