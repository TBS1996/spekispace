use std::{fmt::Debug, sync::Arc, time::Duration};

use dioxus::prelude::*;
use speki_core::Card;
use speki_dto::{AttributeDTO, CardId, RawCard, TimeProvider};
use speki_provider::{DexieProvider, WasmTime};
use speki_web::{Node, NodeMetadata};
use tracing::info;

use crate::{
    firebase::{sign_in, FirestoreProvider},
    nav::SYNCING,
    TouchRec, APP,
};

#[derive(Clone)]
pub struct App(Arc<speki_core::App>);

impl App {
    pub fn inner(&self) -> Arc<speki_core::App> {
        self.0.clone()
    }

    pub async fn delete_card(&self, id: CardId) {
        self.0.card_provider.remove_card(id).await;
    }

    pub async fn fill_cache(&self) {
        self.0.fill_cache().await;
    }

    pub async fn load_all(&self, filter: Option<String>) -> Vec<Arc<Card>> {
        match filter {
            Some(filter) => self.0.cards_filtered(filter).await,
            None => self.0.load_all_cards().await,
        }
    }

    pub async fn load_card(&self, id: CardId) -> Arc<Card> {
        Arc::new(self.0.load_card(id).await.unwrap())
    }

    pub async fn new_from_raw(&self, raw: RawCard) -> Arc<Card> {
        info!("new from raw");
        let card = self.0.new_from_raw(raw).await;
        card
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

impl App {
    pub fn new() -> Self {
        Self(Arc::new(speki_core::App::new(
            speki_core::SimpleRecall,
            WasmTime,
            DexieProvider::new(),
            DexieProvider::new(),
            DexieProvider::new(),
        )))
    }
}

pub async fn sync() {
    let time_provider = APP.read().0.time_provider.clone();
    let agent = sign_in().await;
    info!("starting sync!");

    *SYNCING.write() = true;
    let now = time_provider.current_time();

    speki_dto::sync::<RawCard>(
        FirestoreProvider::new(agent.clone()),
        DexieProvider::new(),
        now,
    )
    .await;
    speki_dto::sync::<speki_dto::History>(
        FirestoreProvider::new(agent.clone()),
        DexieProvider::new(),
        now,
    )
    .await;
    speki_dto::sync::<AttributeDTO>(
        FirestoreProvider::new(agent.clone()),
        DexieProvider::new(),
        now,
    )
    .await;

    *SYNCING.write() = false;
    let elapsed = time_provider.current_time() - now;

    info!("done syncing in {} seconds!", elapsed.as_secs_f32());
}

pub async fn get_meta(node: &Node) -> NodeMetadata {
    match node {
        Node::Card(card_id) => {
            NodeMetadata::from_card(APP.read().load_card(*card_id).await, false).await
        }
        Node::Nope { node, .. } => node.clone(),
    }
}
