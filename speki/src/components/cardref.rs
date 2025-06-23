use std::fmt::Debug;

use dioxus::prelude::*;
use speki_core::{card::CardId, set::SetExpr, Card};
use tracing::info;

use super::CardTy;
use crate::{
    append_overlay,
    overlays::{
        card_selector::{CardSelector, MyClosure},
        OverlayEnum,
    },
    pop_overlay, APP,
};

const PLACEHOLDER: &'static str = "pick card...";

#[derive(PartialEq, Clone)]
pub struct CardRef {
    pub card: Signal<Option<CardId>>,
    pub filter: SetExpr,
    pub allowed: Vec<CardTy>,
    pub on_select: Option<MyClosure>,
    pub on_deselect: Option<MyClosure>,
    pub placeholder: Signal<&'static str>,
}

impl Debug for CardRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CardRef")
            .field("card", &self.card)
            .field("placeholder", &self.placeholder)
            .finish()
    }
}

#[component]
pub fn CardRefRender(
    selected_card: Signal<Option<CardId>>,
    placeholder: &'static str,
    allowed: Vec<CardTy>,
    on_select: Option<MyClosure>,
    on_deselect: Option<MyClosure>,
    #[props(default = SetExpr::All)] filter: SetExpr,
) -> Element {
    let is_selected = selected_card.read().is_some();

    let card_display: Memo<String> = ScopeId::APP.in_runtime(|| {
        use_memo(move || match selected_card.read().as_ref() {
            Some(card_id) => APP.read().load_card(*card_id).read().name().to_string(),
            None => String::new(),
        })
    });

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
                    let fun = MyClosure::new(move |card: Signal<Card>| {
                        info!("x1");
                        let f = f.clone();
                        if let Some(fun) = f.clone() {
                            info!("x2");
                            fun.0(card.clone());
                        }

                        let id = card.read().id();
                        selected_card.clone().set(Some(id));
                        pop_overlay();
                    });

                    let allowed = allowed.clone();
                    let props = CardSelector::ref_picker(fun, filter.clone())
                        .with_allowed_cards(allowed);

                        append_overlay(OverlayEnum::CardSelector(props));
                },
            }
            if is_selected {
                button {
                    class: "absolute top-0 right-0 mt-2 mr-3 ml-6 mb-2 text-gray-500 hover:text-gray-700 focus:outline-none",
                    onclick: move |_| {
                        info!("clicked a button");
                        let on_deselect = on_deselect.clone();
                        if let Some(card) = selected_card.cloned(){
                            let card = APP.cloned().load_card(card);
                            if let Some(f) = on_deselect.clone(){
                                f.call(card);
                            }

                        }

                        selected_card.clone().set(None);
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
            filter: SetExpr::All,
            allowed: vec![],
            on_select: None,
            on_deselect: None,
            placeholder: Signal::new_in_scope(PLACEHOLDER, ScopeId::APP),
        }
    }

    pub fn on_deselect(mut self, f: MyClosure) -> Self {
        self.on_deselect = Some(f);
        self
    }

    pub fn on_select(mut self, f: MyClosure) -> Self {
        self.on_select = Some(f);
        self
    }

    pub fn with_allowed(mut self, deps: Vec<CardTy>) -> Self {
        self.allowed = deps;
        self
    }

    pub fn reset(&self) {
        self.card.clone().set(None);
    }

    pub fn selected_card(&self) -> Signal<Option<CardId>> {
        self.card.clone()
    }

    pub fn set_ref_id(&self, card: CardId) {
        self.card.clone().set(Some(card));
    }

    pub fn set_ref(&self, card: Signal<Card>) {
        let id = card.read().id();
        self.card.clone().set(Some(id));
    }
}
