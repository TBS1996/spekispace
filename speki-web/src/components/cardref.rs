use std::sync::Arc;

use dioxus::prelude::*;
use speki_core::{AnyType, Card};
use speki_dto::CardId;

use super::Komponent;
use crate::{overlays::card_selector, pages::CardEntry, OVERLAY};

const PLACEHOLDER: &'static str = "pick card...";

#[derive(Clone)]
pub struct CardRef {
    card: Signal<Option<CardId>>,
    display: Signal<String>,
    cards: Signal<Vec<CardEntry>>,
}

impl CardRef {
    pub fn new(cards: Signal<Vec<CardEntry>>) -> Self {
        Self {
            card: Signal::new_in_scope(Default::default(), ScopeId(3)),
            display: Signal::new_in_scope(PLACEHOLDER.to_string(), ScopeId(3)),
            cards,
        }
    }

    pub fn reset(&self) {
        self.card.clone().set(None);
        self.display.clone().set(PLACEHOLDER.to_string());
    }

    pub fn selected_card(&self) -> Signal<Option<CardId>> {
        self.card.clone()
    }

    pub fn start_ref_search(&self) {
        let cards = self.cards.clone();

        let _selv = self.clone();

        let fun = move |card: Arc<Card<AnyType>>| {
            let selv = _selv.clone();
            let id = card.id;
            selv.card.clone().set(Some(id));
            spawn(async move {
                let display = card.print().await;
                selv.display.clone().set(display);
            });
        };

        let props = card_selector::CardSelector {
            title: "choose reference".to_string(),
            on_card_selected: Arc::new(Box::new(fun)),
            search: Signal::new_in_scope(Default::default(), ScopeId(3)),
            cards,
            done: Signal::new_in_scope(false, ScopeId(3)),
        };

        OVERLAY.cloned().set(Box::new(props));
    }
}

impl Komponent for CardRef {
    fn render(&self) -> Element {
        let card_display = self.display.clone();
        let selv = self.clone();

        rsx! {
            input {
                class: "w-full border border-gray-300 rounded-md p-2 mb-4 text-gray-950 bg-gray-50 cursor-pointer focus:outline-none",
                value: "{card_display}",
                readonly: "true",
                onclick: move |_| {
                    selv.start_ref_search();
                },
            }
        }
    }
}
