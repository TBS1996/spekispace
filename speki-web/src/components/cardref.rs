use std::sync::Arc;

use dioxus::prelude::*;
use speki_core::{AnyType, Card};
use speki_dto::CardId;

use super::{CardTy, Komponent};
use crate::{
    overlays::{card_selector::CardSelector, cardviewer::TempNode},
    OVERLAY,
};

const PLACEHOLDER: &'static str = "pick card...";

#[derive(Clone)]
pub struct CardRef {
    card: Signal<Option<CardId>>,
    display: Signal<String>,
    filter: Option<Arc<Box<dyn Fn(AnyType) -> bool>>>,
    dependent: Option<TempNode>,
    allowed: Vec<CardTy>,
}

impl Komponent for CardRef {
    fn render(&self) -> Element {
        let card_display = self.display.clone();
        let selv = self.clone();

        rsx! {
            input {
                class: "bg-white w-full border border-gray-300 rounded-md p-2 mb-4 text-gray-950 cursor-pointer focus:outline-none",
                value: "{card_display}",
                readonly: "true",
                onclick: move |_| {
                    selv.start_ref_search();
                },
            }
        }
    }
}

impl CardRef {
    pub fn new() -> Self {
        Self {
            card: Signal::new_in_scope(Default::default(), ScopeId(3)),
            display: Signal::new_in_scope(PLACEHOLDER.to_string(), ScopeId(3)),
            filter: None,
            dependent: None,
            allowed: vec![],
        }
    }

    pub fn with_allowed(mut self, deps: Vec<CardTy>) -> Self {
        self.allowed = deps;
        self
    }

    pub fn with_dependents(mut self, deps: TempNode) -> Self {
        self.dependent = Some(deps);
        self
    }

    pub fn with_filter(mut self, filter: Arc<Box<dyn Fn(AnyType) -> bool>>) -> Self {
        self.filter = Some(filter);
        self
    }

    pub fn reset(&self) {
        self.card.clone().set(None);
        self.display.clone().set(PLACEHOLDER.to_string());
    }

    pub fn selected_card(&self) -> Signal<Option<CardId>> {
        self.card.clone()
    }

    pub async fn set_ref(&self, card: Arc<Card<AnyType>>) {
        let id = card.id;
        self.card.clone().set(Some(id));
        let display = card.print().await;
        self.display.clone().set(display);
    }

    pub fn start_ref_search(&self) {
        let _selv = self.clone();

        let fun = move |card: Arc<Card<AnyType>>| {
            let selv = _selv.clone();
            spawn(async move {
                selv.set_ref(card).await;
            });
        };

        let dependents = self
            .dependent
            .clone()
            .map(|node| vec![node.into()])
            .unwrap_or_default();

        let props =
            CardSelector::ref_picker(Arc::new(Box::new(fun)), dependents, self.filter.clone())
                .with_allowed_cards(self.allowed.clone());

        OVERLAY.cloned().set(Box::new(props));
    }
}
