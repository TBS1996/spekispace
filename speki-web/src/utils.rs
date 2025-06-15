use std::{fmt::Debug, sync::Arc, time::Duration};

use dioxus::prelude::*;
use ledgerstore::TimeProvider;
use speki_core::{card::CardId, Card};
#[cfg(not(feature = "desktop"))]
use speki_provider::{DexieProvider, WasmTime};
use tracing::info;
#[cfg(not(feature = "desktop"))]
use wasm_bindgen::prelude::*;

#[cfg(not(feature = "desktop"))]
use crate::firebase::{AuthUser, FirestoreProvider};

#[derive(Copy, Clone)]
pub struct FsTime;

impl TimeProvider for FsTime {
    fn current_time(&self) -> Duration {
        Duration::from_secs(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        )
    }
}

#[derive(Clone)]
pub struct App(Arc<speki_core::App>);

impl App {
    #[cfg(not(feature = "desktop"))]
    pub fn new() -> Self {
        use speki_dto::Ledger;

        Self(Arc::new(speki_core::App::new(
            speki_core::SimpleRecall,
            WasmTime,
            Ledger::new(Box::new(DexieProvider::new())),
            Ledger::new(Box::new(DexieProvider::new())),
            Ledger::new(Box::new(DexieProvider::new())),
            Ledger::new(Box::new(DexieProvider::new())),
        )))
    }

    #[cfg(feature = "desktop")]
    pub fn new() -> Self {
        use std::path::Path;

        use ledgerstore::Ledger;
        //use speki_provider::{FsProvider, FsTime};
        //let root = Path::new("/home/tor/spekifs/snap4");
        let root = Path::new("/home/tor/spekifs/testing");

        Self(Arc::new(speki_core::App::new(
            speki_core::SimpleRecall,
            FsTime,
            Ledger::new(root),
            Ledger::new(root),
            Ledger::new(root),
            Ledger::new(root),
        )))
    }

    pub fn inner(&self) -> Arc<speki_core::App> {
        self.0.clone()
    }

    pub async fn fill_cache(&self) {
        self.0.fill_index_cache().await;
    }

    pub async fn try_load_card(&self, id: CardId) -> Option<Signal<Card>> {
        self.0
            .load_card(id)
            .await
            .map(|c| Signal::new_in_scope(c, ScopeId::APP))
    }

    pub fn load_card_sync(&self, id: CardId) -> Signal<Card> {
        Signal::new_in_scope(
            self.0
                .load_card_sync(id)
                .expect(&format!("unable to load card with id: {id}")),
            ScopeId::APP,
        )
    }

    pub async fn load_card(&self, id: CardId) -> Signal<Card> {
        Signal::new_in_scope(
            self.0
                .load_card(id)
                .await
                .expect(&format!("unable to load card with id: {id}")),
            ScopeId::APP,
        )
    }

    pub async fn new_instance(
        &self,
        front: String,
        back: Option<String>,
        class: CardId,
    ) -> Arc<Card> {
        info!("new simple");
        let id = self.0.add_instance(front, back, class).await;
        let card = Arc::new(self.0.load_card(id).await.unwrap());
        card
    }

    pub async fn new_simple(&self, front: String, back: String) -> Arc<Card> {
        info!("new simple");
        let id = self.0.add_card(front, back).await;
        let card = Arc::new(self.0.load_card(id).await.unwrap());
        card
    }
}

impl Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("App").field(&self.0).finish()
    }
}

#[cfg(not(feature = "desktop"))]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = Date)]
    fn now() -> f64;
}
