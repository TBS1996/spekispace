use std::{fmt::Debug, sync::Arc};

use dioxus::prelude::*;
use ledgerstore::Ledger;
use speki_core::{card::CardId, Card};
use tracing::info;

#[derive(Clone)]
pub struct App(Arc<speki_core::App>);

impl App {
    pub fn new() -> Self {
        let root = dirs::data_local_dir().unwrap().join("speki");

        Self(Arc::new(speki_core::App::new(
            Ledger::new(root.clone()),
            Ledger::new(root.clone()),
            Ledger::new(root.clone()),
            Ledger::new(root.clone()),
        )))
    }

    pub fn inner(&self) -> Arc<speki_core::App> {
        self.0.clone()
    }

    pub fn try_load_card(&self, id: CardId) -> Option<Signal<Card>> {
        self.0
            .load_card(id)
            .map(|c| Signal::new_in_scope(c, ScopeId::APP))
    }

    pub fn load_card(&self, id: CardId) -> Signal<Card> {
        Signal::new_in_scope(
            self.0
                .load_card(id)
                .expect(&format!("unable to load card with id: {id}")),
            ScopeId::APP,
        )
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
