use std::{
    collections::{BTreeSet, HashMap, HashSet}, sync::{atomic::{AtomicU64, Ordering}, Arc}, time::Duration
};
use async_trait::async_trait;
use js_sys::Promise;
use serde_json::Value;
use speki_dto::{LedgerEntry, LedgerEvent, ProviderId, SpekiProvider, TimeProvider};
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
impl<T: RunLedger<L>, L: LedgerEvent> SpekiProvider<T, L> for DexieProvider {
    async fn current_time(&self) -> Duration {
        self.time.current_time()
    }

    async fn save_cache(&self, key: String, ids: HashSet<String>) {
        let value = serde_wasm_bindgen::to_value(&ids).unwrap(); // Store as a native array
        let id = JsValue::from_str(&key);
        let space = format!("{}_cache", T::identifier());
        saveContent(&JsValue::from_str(&space), &id, &value);
    }

    async fn load_cache(&self, key: &str) -> HashSet<String>{
        let space = format!("{}_cache", T::identifier());
        let id = JsValue::from_str(key);
        let promise = loadRecord(&JsValue::from_str(&space), &id);
        let future = wasm_bindgen_futures::JsFuture::from(promise);
        let jsvalue = future.await.unwrap();
        let x: Option<HashSet<String>> = serde_wasm_bindgen::from_value(jsvalue).unwrap();
        x.unwrap_or_default()
    }

    async fn load_content(&self, space: &str, id: &str) -> Option<String> {
        let ty = T::identifier();
        let id = JsValue::from_str(&id.to_string());
        let promise = loadRecord(&JsValue::from_str(ty), &id);
        let future = wasm_bindgen_futures::JsFuture::from(promise);
        let jsvalue = future.await.unwrap();
        serde_wasm_bindgen::from_value(jsvalue).unwrap()
    }

    async fn load_all_contents(&self) -> HashMap<String, String> {
        let ty = T::identifier();
        info!("from dexie loading all {ty:?}");
        let promise = loadAllRecords(&JsValue::from_str(ty));
        let future = wasm_bindgen_futures::JsFuture::from(promise);
        let jsvalue = future.await.unwrap();
        let map: HashMap<String, String> = serde_wasm_bindgen::from_value(jsvalue).unwrap();
        info!("from dexie loaded {:?} {:?}", map.len(), ty);
        map
    }

    async fn save_content(&self, space: &str, id: String, record: String) {
        let id = JsValue::from_str(&id);
        let content = JsValue::from_str(&record);
        saveContent(&JsValue::from_str(space), &id, &content);
    }

    async fn load_ids(&self) -> Vec<String> {
        info!("loaidng all ids of type: {}", T::identifier());
        load_ids(T::identifier()).await
    }


    async fn load_ledger(&self) -> Vec<L>{
        let space = format!("{}_ledger", T::identifier());

        let promise = loadAllRecords(&JsValue::from_str(&space));
        let future = wasm_bindgen_futures::JsFuture::from(promise);
        let jsvalue = future.await.unwrap();
        let map: HashMap<String, String> = serde_wasm_bindgen::from_value(jsvalue).unwrap();

        let mut foo: Vec<LedgerEntry<L>> = vec![];

        for (key, value) in map.iter(){
            let key: u64 = key.parse().unwrap();
            let action: L = serde_json::from_str(&value).unwrap();
            let timestamp = Duration::from_micros(key + (MICRO.fetch_add(1, Ordering::SeqCst) % 1_000_000));
            let event: LedgerEntry<L> = LedgerEntry::new(timestamp, action);
            foo.push(event);
        }

        foo.sort_by_key(|k|k.timestamp);
        foo.into_iter().map(|e| e.event).collect()
    }

    /// Clear the storage area so we can re-run everything.
    async fn clear_state(&self) {
        let space = JsValue::from_str(T::identifier());
        clearSpace(&space);
    }

    async fn clear_space(&self, _space: &str) {
        unreachable!()
    }

    async fn clear_ledger(&self) {
        let space = format!("{}_ledger", T::identifier());
        let space = JsValue::from_str(&space);
        clearSpace(&space);
    }


    async fn save_ledger(&self, event: LedgerEntry<L>) {
        let space = format!("{}_ledger", T::identifier());
        let id = JsValue::from_str(&event.timestamp.as_micros().to_string());
        let content = JsValue::from_str(&serde_json::to_string(&event.event).unwrap());
        saveContent(&JsValue::from_str(&space), &id, &content);
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
