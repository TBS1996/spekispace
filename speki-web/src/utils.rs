use std::{fmt::Debug, sync::Arc};

use dioxus::prelude::*;
use speki_core::{
    card::CardId,
    collection::{Collection, CollectionId},
    Card,
};
#[cfg(not(feature = "desktop"))]
use speki_provider::{DexieProvider, WasmTime};
use speki_web::{Node, NodeMetadata};
use tracing::info;
#[cfg(not(feature = "desktop"))]
use wasm_bindgen::prelude::*;

use crate::APP;

#[cfg(not(feature = "desktop"))]
use crate::firebase::{AuthUser, FirestoreProvider};

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
        use speki_provider::FsTime;
        //use speki_provider::{FsProvider, FsTime};
        let root = Path::new("/home/tor/spekifs/snap4");
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

    pub async fn delete_card(&self, id: CardId) {
        self.0.card_provider.remove_card(id).await;
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

    pub async fn load_collection(&self, id: CollectionId) -> Collection {
        self.0.provider.collections.load(&id.to_string()).unwrap()
    }

    pub async fn load_collections(&self) -> Vec<Collection> {
        self.0
            .provider
            .collections
            .load_all()
            .into_values()
            .collect()
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

/// **Check if an element is present (Web & Desktop)**
pub fn is_element_present(id: &str) -> bool {
    #[cfg(feature = "web")]
    {
        return web_sys::window()
            .and_then(|win| win.document())
            .unwrap()
            .get_element_by_id(id)
            .is_some();
    }

    #[cfg(feature = "desktop")]
    {
        return true;
    }
    panic!()
}

#[cfg(not(feature = "desktop"))]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = Date)]
    fn now() -> f64;
}

/*
pub async fn sync(agent: AuthUser) {
    let time_provider = APP.read().0.time_provider.clone();
    info!("starting sync!");

    *SYNCING.write() = true;
    let now = time_provider.current_time();

    let fire = async {
        let mut fire = FirestoreProvider::new(agent);
        let id = Syncable::<BaseCard>::provider_id(&fire).await;
        fire.set_id(id);
        fire
    };

    let dex = async {
        let mut dex = DexieProvider::new();
        let id = Syncable::<BaseCard>::provider_id(&dex).await;
        dex.set_id(id);
        dex
    };

    let (fire, dex) = join(fire, dex).await;

    let cardsync = Syncable::<BaseCard>::sync(fire.clone(), dex.clone());
    let revsync = Syncable::<speki_core::recall_rate::History>::sync(fire.clone(), dex.clone());
    let attrsync = Syncable::<AttributeDTO>::sync(fire.clone(), dex.clone());
    let colsync = Syncable::<Collection>::sync(fire.clone(), dex.clone());
    let metasync = Syncable::<Metadata>::sync(fire.clone(), dex.clone());
    let filtersync = Syncable::<FilterItem>::sync(fire.clone(), dex.clone());
    let audiosync = Syncable::<Audio>::sync(fire.clone(), dex.clone());

    futures::future::join_all(vec![
        cardsync, revsync, attrsync, colsync, metasync, filtersync, audiosync,
    ])
    .await;

    *SYNCING.write() = false;
    let elapsed = time_provider.current_time() - now;

    info!("done syncing in {} seconds!", elapsed.as_secs_f32());
}
    */

pub async fn get_meta(node: &Node) -> NodeMetadata {
    match node {
        Node::Card(card_id) => {
            NodeMetadata::from_card(APP.read().load_card(*card_id).await, false).await
        }
        Node::Nope { node, .. } => node.clone(),
    }
}
