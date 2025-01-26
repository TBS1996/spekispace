use std::{fmt::Debug, sync::Arc};

use dioxus::prelude::*;
use speki_core::{card::CardId, Card, CardType};
use tracing::info;

use super::CardTy;
use crate::{
    overlays::{
        card_selector::{CardSelector, MyClosure},
        cardviewer::TempNode,
        OverlayEnum,
    },
    APP,
};

const PLACEHOLDER: &'static str = "pick card...";

#[derive(PartialEq, Clone)]
pub struct CardRef {
    pub card: Signal<Option<CardId>>,
    pub display: Signal<String>,
    pub filter: Option<Callback<CardType, bool>>,
    pub dependent: Option<TempNode>,
    pub allowed: Vec<CardTy>,
    pub on_select: Option<MyClosure>,
    pub on_deselect: Option<MyClosure>,
    pub placeholder: Signal<&'static str>,
}

impl Debug for CardRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CardRef")
            .field("card", &self.card)
            .field("display", &self.display)
            .field("placeholder", &self.placeholder)
            .finish()
    }
}

#[component]
pub fn CardRefRender(
    card_display: Signal<String>,
    selected_card: Signal<Option<CardId>>,
    placeholder: &'static str,
    on_select: Option<MyClosure>,
    on_deselect: Option<MyClosure>,
    dependent: Option<TempNode>,
    filter: Option<Callback<CardType, bool>>,
    allowed: Vec<CardTy>,
    overlay: Signal<Option<OverlayEnum>>,
) -> Element {
    let is_selected = selected_card.read().is_some();

    rsx! {
        div {
            class: "relative w-full",
            input {
                class: "bg-white w-full border border-gray-300 rounded-md p-2 mb-2 text-gray-950 cursor-pointer focus:outline-none",
                placeholder: "{placeholder}",
                value: "{card_display}",
                readonly: "true",
                onclick: move |_| {
                    let f = on_select.clone();
                    let fun = MyClosure::new(move |card: Arc<Card>| {
                        info!("x1");
                        let f = f.clone();
                        async move {
                        if let Some(fun) = f.clone() {
                            info!("x2");
                            fun.0(card.clone()).await;
                        }

                        let id = card.id();
                        selected_card.clone().set(Some(id));
                        let display = card.print().await;
                        card_display.clone().set(display);
                        }
                    });

                    let dependents = dependent
                        .clone()
                        .map(|node| vec![node.into()])
                        .unwrap_or_default();

                    let filter = filter.clone();
                    let allowed = allowed.clone();
                    spawn(async move {
                        let props = CardSelector::ref_picker(fun, dependents, filter)
                            .await
                            .with_allowed_cards(allowed);

                        overlay.clone().set(Some(OverlayEnum::CardSelector(props)));
                    });
                },
            }
            if is_selected {
                button {
                    class: "absolute top-0 right-0 mt-2 mr-3 ml-6 mb-2 text-gray-500 hover:text-gray-700 focus:outline-none",
                    onclick: move |_| {
                        info!("clicked a button");
                        let on_deselect = on_deselect.clone();
                        spawn(async move {
                            if let Some(card) = selected_card.cloned(){
                                let card = APP.cloned().load_card(card).await;
                                if let Some(f) = on_deselect.clone(){
                                    f.call(card).await;
                                }

                            }

                            selected_card.clone().set(None);
                            card_display.clone().set(Default::default());
                        });
                    },
                    "X",
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

    pub fn with_deselect(mut self, f: MyClosure) -> Self {
        self.on_select = Some(f);
        self
    }

    pub fn with_closure(mut self, f: MyClosure) -> Self {
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

    pub fn with_filter(mut self, filter: Callback<CardType, bool>) -> Self {
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

    pub async fn set_ref(&self, card: Arc<Card>) {
        let id = card.id();
        self.card.clone().set(Some(id));
        let display = card.print().await;
        self.display.clone().set(display);
    }
}
