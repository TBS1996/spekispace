use std::{fmt::Display, sync::Arc};

use dioxus::prelude::*;

use crate::components::Komponent;

use super::Overlay;

#[derive(Clone)]
pub struct ItemSelector<T: Display + Clone> {
    items: Vec<T>,
    on_selected: Arc<Box<dyn Fn(T)>>,
    done: Signal<bool>,
}

impl<T: Display + Clone> ItemSelector<T> {
    pub fn new(items: Vec<T>, on_selected: Arc<Box<dyn Fn(T)>>) -> Self {
        Self {
            items,
            on_selected,
            done: Signal::new_in_scope(false, ScopeId::APP),
        }
    }
}

impl<T: Display + Clone + 'static> Komponent for ItemSelector<T> {
    fn render(&self) -> Element {
        let mut iter: Vec<(T, Arc<Box<dyn Fn(T)>>, Signal<bool>)> = vec![];

        for item in self.items.clone() {
            iter.push((item, self.on_selected.clone(), self.done.clone()));
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
}

impl<T: Display + Clone + 'static> Overlay for ItemSelector<T> {
    fn is_done(&self) -> Signal<bool> {
        self.done.clone()
    }
}
