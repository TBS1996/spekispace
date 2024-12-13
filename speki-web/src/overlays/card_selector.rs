use std::{rc::Rc, sync::Arc};

use dioxus::prelude::*;
use speki_core::{AnyType, Card};

use crate::{pages::CardEntry, Komponent, PopTray};

#[derive(Props, Clone)]
pub struct CardSelector {
    pub title: String,
    pub search: Signal<String>,
    pub on_card_selected: Rc<dyn Fn(Arc<Card<AnyType>>)>,
    pub cards: Signal<Vec<CardEntry>>,
    pub done: Signal<bool>,
}

impl Komponent for CardSelector {
    /// Selects a card from the collection and calls a closure on it.
    fn render(&self) -> Element {
        let title = &self.title;
        let mut search = self.search.clone();

        let closure = Arc::new(self.on_card_selected.clone());

        let filtered_cards: Vec<_> = self
            .cards
            .iter()
            .filter(|card| card.front.contains(&search.cloned()))
            .take(50)
            .zip(std::iter::repeat_with(|| Arc::clone(&closure)))
            .map(|(card, closure)| (card.clone(), closure, self.done.clone()))
            .collect();

        rsx! {
            h1 { "{title}" }

            input {
                class: "w-full border border-gray-300 rounded-md p-2 mb-4 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                value: "{search}",
                oninput: move |evt| search.set(evt.value().clone()),
            }

            table {
                class: "min-w-full table-fixed border-collapse border border-gray-200",
                thead {
                    class: "bg-gray-500",
                    tr {
                        th { class: "border border-gray-300 px-4 py-2 w-2/3", "Front" }
                        th { class: "border border-gray-300 px-4 py-2 w-1/12", "Recall" }
                        th { class: "border border-gray-300 px-4 py-2 w-1/12", "Stability" }
                    }
                }
                tbody {
                    for (card, _closure, is_done) in filtered_cards {
                        tr {
                            class: "hover:bg-gray-50",
                            onclick: move |_| {
                                let card = card.clone();
                                let closure = _closure.clone();
                                let done = is_done.clone();
                                spawn(async move {
                                    closure(card.card.clone());
                                });

                                done.clone().set(true);

                            },

                            td { class: "border border-gray-300 px-4 py-2 w-2/3", "{card.front}" }
                            td { class: "border border-gray-300 px-4 py-2 w-1/12", "{card.card.recall_rate().unwrap_or_default():.2}" }
                            td { class: "border border-gray-300 px-4 py-2 w-1/12", "{card.card.maybeturity().unwrap_or_default():.1}" }
                        }
                    }
                }
            }
        }
    }
}

impl PopTray for CardSelector {
    fn is_done(&self) -> Signal<bool> {
        self.done.clone()
    }
}

impl PartialEq for CardSelector {
    fn eq(&self, other: &Self) -> bool {
        self.title == other.title && self.search == other.search
    }
}
