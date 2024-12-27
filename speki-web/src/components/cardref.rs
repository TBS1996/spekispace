use std::sync::Arc;

use dioxus::prelude::*;
use speki_core::{AnyType, Card};
use speki_dto::CardId;
use tracing::info;

use super::{CardTy, Komponent};
use crate::{
    overlays::{card_selector::CardSelector, cardviewer::TempNode},
    APP, OVERLAY,
};

const PLACEHOLDER: &'static str = "pick card...";

#[derive(Clone)]
pub struct CardRef {
    card: Signal<Option<CardId>>,
    display: Signal<String>,
    filter: Option<Arc<Box<dyn Fn(AnyType) -> bool>>>,
    dependent: Option<TempNode>,
    allowed: Vec<CardTy>,
    on_select: Option<Arc<Box<dyn Fn(Arc<Card<AnyType>>)>>>,
    on_deselect: Option<Arc<Box<dyn Fn(Arc<Card<AnyType>>)>>>,
    placeholder: Signal<&'static str>,
}

impl Komponent for CardRef {
    fn render(&self) -> Element {
        let card_display = self.display.clone();
        let selv = self.clone();
        let selv2 = self.clone();

        let is_selected = self.card.read().is_some();
        let placeholder = self.placeholder.clone();

        rsx! {
            div {
                class: "relative w-full",
                // Container to position the input and the button
                input {
                    class: "bg-white w-full border border-gray-300 rounded-md p-2 mb-2 text-gray-950 cursor-pointer focus:outline-none",
                    placeholder: "{placeholder()}",
                    value: "{card_display}",
                    readonly: "true",
                    onclick: move |_| {
                        selv.start_ref_search();
                    },
                }
                if is_selected {
                    button {
                        class: "absolute top-0 right-0 mt-2 mr-3 ml-6 mb-2 text-gray-500 hover:text-gray-700 focus:outline-none",
                        onclick: move |_| {
                            info!("clicked a button");
                            let selv2 = selv2.clone();
                            spawn(async move {
                                if let Some(card) = selv2.card.cloned(){
                                    let card = APP.cloned().load_card(card).await;
                                    if let Some(f) = selv2.on_deselect.clone(){
                                        f(card);
                                    }

                                }
                                selv2.reset();
                            });
                        },
                        "X",
                    }
                }
            }
        }
    }
}

impl CardRef {
    pub fn new() -> Self {
        Self {
            card: Signal::new_in_scope(Default::default(), ScopeId(3)),
            display: Signal::new_in_scope(Default::default(), ScopeId(3)),
            filter: None,
            dependent: None,
            allowed: vec![],
            on_select: None,
            on_deselect: None,
            placeholder: Signal::new_in_scope(PLACEHOLDER, ScopeId::APP),
        }
    }

    pub fn with_placeholder(self, placeholder: &'static str) -> Self {
        self.placeholder.clone().set(placeholder);
        self
    }

    pub fn with_deselect(mut self, f: Arc<Box<dyn Fn(Arc<Card<AnyType>>)>>) -> Self {
        self.on_deselect = Some(f);
        self
    }

    pub fn with_closure(mut self, f: Arc<Box<dyn Fn(Arc<Card<AnyType>>)>>) -> Self {
        self.on_select = Some(f);
        self
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
        self.display.clone().set(Default::default());
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

        let f = self.on_select.clone();
        let fun = move |card: Arc<Card<AnyType>>| {
            let selv = _selv.clone();
            if let Some(fun) = f.clone() {
                fun(card.clone());
            }

            spawn(async move {
                selv.set_ref(card).await;
            });
        };

        let dependents = self
            .dependent
            .clone()
            .map(|node| vec![node.into()])
            .unwrap_or_default();

        let filter = self.filter.clone();
        let allowed = self.allowed.clone();
        spawn(async move {
            let props = CardSelector::ref_picker(Arc::new(Box::new(fun)), dependents, filter)
                .await
                .with_allowed_cards(allowed);

            OVERLAY.cloned().set(Box::new(props));
        });
    }
}
