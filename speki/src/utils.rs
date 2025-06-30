use std::{fmt::Debug, sync::Arc};

use dioxus::prelude::*;
use ledgerstore::EventError;
use speki_core::{
    card::{CardId, RawCard},
    recall_rate::Recall,
    Card, CardRefType,
};
use tracing::info;

use crate::{
    append_overlay,
    overlays::{notice::Notice, OverlayEnum},
    APP,
};

#[derive(Clone)]
pub struct App(Arc<speki_core::App>);

impl App {
    pub fn new() -> Self {
        let args: Vec<String> = std::env::args().collect();

        let root = if args.get(1).is_some_and(|arg| arg == "debug") {
            let path = dirs::data_local_dir().unwrap().join("speki_debug");
            // creating a fresh root for debugging
            let _ = std::fs::remove_dir_all(&path);
            path
        } else if args.get(1).is_some_and(|arg| arg == "debug_persist") {
            dirs::data_local_dir().unwrap().join("speki_debug")
        } else {
            dirs::data_local_dir().unwrap().join("speki")
        };

        Self(Arc::new(speki_core::App::new(root)))
    }

    pub fn inner(&self) -> Arc<speki_core::App> {
        self.0.clone()
    }

    pub fn try_load_card_signal(&self, id: CardId) -> Option<Signal<Card>> {
        self.0
            .load_card(id)
            .map(|c| Signal::new_in_scope(c, ScopeId::APP))
    }

    pub fn try_load_card(&self, id: CardId) -> Option<Arc<Card>> {
        self.0.load_card(id).map(Arc::new)
    }

    pub fn load_card(&self, id: CardId) -> Arc<Card> {
        self.try_load_card(id)
            .expect(&format!("unable to load card with id: {id}"))
    }

    pub fn new_instance(&self, front: String, back: Option<String>, class: CardId) -> Arc<Card> {
        info!("new simple");
        let id = self.0.add_instance(front, back, class);
        let card = Arc::new(self.0.load_card(id).unwrap());
        card
    }

    pub fn new_simple(&self, front: String, back: String) -> Arc<Card> {
        info!("new simple");
        let id = self.0.add_card(front, back);
        let card = Arc::new(self.0.load_card(id).unwrap());
        card
    }
}

impl Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("App").field(&self.0).finish()
    }
}

pub fn handle_card_event_error(err: EventError<RawCard>) {
    let provider = APP.read().inner().card_provider.clone();

    let text = match err {
        EventError::Cycle(items) => {
            dbg!(&items);
            let mut foobar: Vec<(CardId, CardId, CardRefType)> = vec![];

            for i in 0..items.len() {
                let (to, ref_type) = &items[i];
                let from = if i == 0 {
                    items.last().unwrap().0.clone() // wraparound
                } else {
                    items[i - 1].0.clone()
                };
                foobar.push((from, to.clone(), ref_type.clone()));
            }

            let mut s = format!("cycle detected!\n");
            for (from, to, ty) in foobar {
                let from = provider.load(from).unwrap().name().to_string();
                let to = provider.load(to).unwrap().name().to_string();
                use speki_core::CardRefType as TY;

                let line = match ty {
                    TY::ExplicitDependency => format!("{from} depends on {to}"),
                    TY::ClassOfInstance => format!("{from} is an instance of {to} "),
                    TY::LinkRef => format!("{from} links to {to}"),
                    TY::ParentClass => format!("{from} is a parent class of {to}"),
                    TY::InstanceOfAttribute => format!("{from} is an instance of attribute {to}"),
                };
                s.push_str(&line);
                s.push_str("\n");
            }
            s
        }
        EventError::Invariant(inv) => format!("invariant broken: {inv:?}"),
        EventError::ItemNotFound => format!("card not found"),
        EventError::DeletingWithDependencies => format!("cannot delete card with dependencies"),
    };

    let notice = Notice::new(text);
    let overlay = OverlayEnum::Notice(notice);
    append_overlay(overlay);
}

pub fn recall_to_emoji(recall: Recall) -> &'static str {
    match recall {
        Recall::None => "😞",
        Recall::Late => "😐",
        Recall::Some => "🙂",
        Recall::Perfect => "😃",
    }
}
