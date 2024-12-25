use std::{fmt::Debug, sync::Arc, time::Duration};

use speki_core::{AnyType, Card, TimeProvider};
use speki_dto::{CardId, RawCard};
use speki_provider::DexieProvider;
use speki_web::{Node, NodeMetadata};
use tracing::info;

use crate::{
    firebase::{sign_in, FirestoreProvider},
    nav::SYNCING,
    pages::CardEntry,
    TouchRec, APP, CARDS,
};

#[derive(Clone)]
pub struct App(Arc<speki_core::App>);

impl App {
    pub fn inner(&self) -> Arc<speki_core::App> {
        self.0.clone()
    }

    pub async fn fill_cache(&self) {
        self.0.fill_cache().await;
    }

    pub async fn load_card(&self, id: CardId) -> Arc<Card<AnyType>> {
        Arc::new(self.0.load_card(id).await.unwrap())
    }

    pub async fn load_non_pending(&self, filter: Option<String>) -> Vec<CardId> {
        self.0.load_non_pending(filter).await
    }

    pub async fn new_from_raw(&self, raw: RawCard) -> Arc<Card<AnyType>> {
        let card = self.0.new_from_raw(raw).await;
        CARDS.read().insert(card.clone()).await;
        card
    }

    pub async fn new_simple(&self, front: String, back: String) -> Arc<Card<AnyType>> {
        let id = self.0.add_card(front, back).await;
        let card = Arc::new(self.0.load_card(id).await.unwrap());
        CARDS.read().insert(card.clone()).await;
        card
    }
}

impl Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("App").field(&self.0).finish()
    }
}

pub fn rect(id: &str) -> Option<TouchRec> {
    let rec = web_sys::window()?
        .document()?
        .get_element_by_id(id)?
        .get_bounding_client_rect();

    let rect = TouchRec {
        x: rec.x(),
        y: rec.y(),
        height: rec.height(),
        width: rec.width(),
    };
    info!("rect is {rect:?}");

    Some(rect)
}

pub fn is_element_present(id: &str) -> bool {
    web_sys::window()
        .and_then(|win| win.document())
        .unwrap()
        .get_element_by_id(id)
        .is_some()
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

    *SYNCING.write() = true;
    DexieProvider.sync(fsp).await;
    *SYNCING.write() = false;

    info!("done syncing maybe!");
}

#[derive(Clone, Default)]
pub struct CardEntries {
    pub cards: Signal<Vec<CardEntry>>,
    pub classes: Signal<Vec<CardEntry>>,
}

impl CardEntries {
    pub async fn insert(&self, card: Arc<Card<AnyType>>) {
        let entry = CardEntry::new(card.clone()).await;

        if card.is_class() {
            self.classes.clone().write().push(entry.clone());
        }

        self.cards.clone().write().push(entry);
    }

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

pub async fn get_meta(node: &Node) -> NodeMetadata {
    match node {
        Node::Card(card_id) => {
            NodeMetadata::from_card(APP.read().load_card(*card_id).await, false).await
        }
        Node::Nope { node, .. } => node.clone(),
    }
}
