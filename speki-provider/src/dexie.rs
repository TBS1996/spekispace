use std::{
    collections::{BTreeSet, HashMap, HashSet}, sync::{atomic::{AtomicU64, Ordering}, Arc}, time::Duration
};
use async_trait::async_trait;
use js_sys::Promise;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use speki_dto::{LedgerEntry, LedgerEvent, LedgerProvider, ProviderId, Storage, TimeProvider};
use tracing::info;
use wasm_bindgen::prelude::*;
use speki_dto::RunLedger;

#[derive(Copy, Clone)]
pub struct WasmTime;

impl TimeProvider for WasmTime {
    fn current_time(&self) -> Duration {
        Duration::from_millis(now() as u64)
    }
}

#[derive(Clone)]
pub struct DexieProvider {
    time: WasmTime,
    id: Option<ProviderId>,
}

impl DexieProvider {
    pub fn new() -> Self {
        Self {
            time: WasmTime,
            id: None,
        }
    }

    pub fn set_id(&mut self, id: ProviderId) {
        self.id = Some(id);
    }
}

#[async_trait(?Send)]
impl<T: Serialize + DeserializeOwned + 'static> Storage<T> for DexieProvider {
    async fn load_content(&self, space: &str, id: &str) -> Option<String> {
        let id = JsValue::from_str(&id);
        let promise = loadRecord(&JsValue::from_str(space), &id);
        let future = wasm_bindgen_futures::JsFuture::from(promise);
        let jsvalue = future.await.unwrap();
        serde_wasm_bindgen::from_value(jsvalue).unwrap()
    }

    async fn load_all_contents(&self, space: &str) -> HashMap<String, String> {
        info!("from dexie loading all {space:?}");
        let promise = loadAllRecords(&JsValue::from_str(space));
        let future = wasm_bindgen_futures::JsFuture::from(promise);
        let jsvalue = future.await.unwrap();
        let map: HashMap<String, String> = serde_wasm_bindgen::from_value(jsvalue).unwrap();
        info!("from dexie loaded {:?} {:?}", map.len(), space);
        map
    }

    async fn save_content(&self, space: &str, id: &str, record: String) {
        let id = JsValue::from_str(&id);
        let content = JsValue::from_str(&record);
        saveContent(&JsValue::from_str(space), &id, &content);
    }

    async fn load_ids(&self, space: &str) -> Vec<String> {
        info!("loaidng all ids of type: {space}");
        load_ids(space).await
    }
}


#[async_trait(?Send)]
impl<T: RunLedger<L>, L: LedgerEvent> LedgerProvider<T, L> for DexieProvider {
    async fn current_time(&self) -> Duration {
        self.time.current_time()
    }

    async fn clear_space(&self, space: &str) {
        let space = JsValue::from_str(&space);
        clearSpace(&space);
    }
}


static MICRO: once_cell::sync::Lazy<Arc<AtomicU64>> = once_cell::sync::Lazy::new(|| {
    Arc::new(AtomicU64::new(0))
});

#[wasm_bindgen(module = "/dexie.js")]
extern "C" {
    fn loadDbId() -> Promise;
    fn saveDbId(id: &JsValue);

    fn saveSyncTime(key: &JsValue, lastSync: &JsValue);
    fn loadSyncTime(key: &JsValue) -> Promise;

    fn loadRecord(table: &JsValue, id: &JsValue) -> Promise;
    fn loadAllRecords(table: &JsValue) -> Promise;
    fn saveContent(table: &JsValue, id: &JsValue, content: &JsValue);
    fn deleteContent(table: &JsValue, id: &JsValue);
    fn loadAllIds(table: &JsValue) -> Promise;

    fn clearSpace(table: &JsValue);
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = Date)]
    fn now() -> f64;
}

pub fn _current_time() -> Duration {
    Duration::from_millis(now() as u64)
}

pub async fn load_ids(table: &str) -> Vec<String> {
    info!("load_ids called, table: {}", table);
    let val = promise_to_val(loadAllIds(&JsValue::from_str(table))).await;
    val.as_array()
        .unwrap()
        .into_iter()
        .map(|obj| serde_json::from_value(obj.clone()).unwrap())
        .collect()
}

async fn promise_to_val(promise: Promise) -> Value {
    let future = wasm_bindgen_futures::JsFuture::from(promise);
    let jsvalue = future.await.unwrap();
    jsvalue.into_serde().unwrap()
}
