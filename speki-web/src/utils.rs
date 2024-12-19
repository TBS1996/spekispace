use std::{fmt::Debug, sync::Arc, time::Duration};

use speki_core::TimeProvider;
use speki_provider::DexieProvider;
use tracing::info;

use crate::{
    firebase::{sign_in, FirestoreProvider},
    pages::CardEntry,
    APP,
};

#[derive(Clone)]
pub struct App(pub Arc<speki_core::App>);

impl AsRef<speki_core::App> for App {
    fn as_ref(&self) -> &speki_core::App {
        &self.0
    }
}

impl Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("App").field(&self.0).finish()
    }
}

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = Date)]
    fn now() -> f64;
}

struct WasmTime;

impl TimeProvider for WasmTime {
    fn current_time(&self) -> Duration {
        Duration::from_millis(now() as u64)
    }
}

impl App {
    pub fn new() -> Self {
        Self(Arc::new(speki_core::App::new(
            DexieProvider,
            speki_core::SimpleRecall,
            WasmTime,
        )))
    }
}

pub async fn sync() {
    use speki_dto::SpekiProvider;

    let agent = sign_in().await;
    info!("starting sync!");

    let fsp: Box<dyn SpekiProvider> = Box::new(FirestoreProvider::new(agent));

    DexieProvider.sync(fsp).await;

    info!("done syncing maybe!");
}

#[derive(Clone, Default)]
pub struct CardEntries {
    pub cards: Signal<Vec<CardEntry>>,
    pub classes: Signal<Vec<CardEntry>>,
}

impl CardEntries {
    pub async fn fill(&self) {
        let app = APP.cloned();
        let mut concept_cards = vec![];
        let mut cards = vec![];

        for card in app.0.load_all_cards().await {
            if card.is_class() {
                concept_cards.push(CardEntry::new(card.clone()).await);
            }
            cards.push(CardEntry::new(card).await);
        }

        self.cards.clone().set(cards);
        self.classes.clone().set(concept_cards);
    }
}

use dioxus::prelude::*;
