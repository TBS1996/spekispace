use std::{
    collections::{BTreeSet, HashMap}, sync::{atomic::{AtomicU64, Ordering}, Arc}, time::Duration
};
use async_trait::async_trait;
use js_sys::Promise;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use speki_dto::Indexable;
use speki_dto::{Item, LedgerEntry, LedgerEvent, LedgerProvider, ProviderId, Record, SpekiProvider, Syncable, TimeProvider};
use tracing::info;
use uuid::Uuid;
use wasm_bindgen::prelude::*;
use speki_dto::RunLedger;
use std::hash::Hash;

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
impl<T: Item> Indexable<T> for DexieProvider {
    async fn load_indices(&self, word: String) -> BTreeSet<Uuid> {
        info!("loading indices for: {word}");
        let ty = format!("textindex_{}", T::identifier());
        let id = JsValue::from_str(&word);
        let promise = loadRecord(&JsValue::from_str(&ty), &id);
        let future = wasm_bindgen_futures::JsFuture::from(promise);
        let jsvalue = future.await.unwrap();
        let record: Option<Record> = serde_wasm_bindgen::from_value(jsvalue).unwrap_or_default();
        record
            .map(|r| serde_json::from_str(&r.content).unwrap())
            .unwrap_or_default()
    }

    async fn save_indices(&self, word: String, indices: BTreeSet<Uuid>) {
        info!("saving indices for: {word}");
        let ty = format!("textindex_{}", T::identifier());
        let id = JsValue::from_str(&word);
        let content = JsValue::from_str(&serde_json::to_string(&indices).unwrap());
        let last_modified = JsValue::from_str(&0.to_string());
        saveContent(&JsValue::from_str(&ty), &id, &content, &last_modified);
    }
}

#[async_trait(?Send)]
impl<T: Item> Syncable<T> for DexieProvider {
    async fn save_id(&self, id: ProviderId) {
        let s = JsValue::from_str(&id.to_string());
        saveDbId(&s);
    }

    async fn load_id_opt(&self) -> Option<ProviderId> {
        if self.id.is_some() {
            return self.id;
        }

        let promise = loadDbId();
        let future = wasm_bindgen_futures::JsFuture::from(promise);
        let jsvalue = future.await.unwrap();
        serde_wasm_bindgen::from_value::<ProviderId>(jsvalue).ok()
    }

    async fn update_sync_info(&self, other: ProviderId, current_time: Duration) {
        let ty = T::identifier();
        let key = format!("{}-{:?}", other, ty);
        let key = JsValue::from_str(&key);
        let val = JsValue::from_f64(current_time.as_secs() as f64);
        saveSyncTime(&key, &val);
    }

    async fn last_sync(&self, other: ProviderId) -> Duration {
        let ty = T::identifier();
        let key = format!("{}-{:?}", other, ty);
        let key = JsValue::from_str(&key);
        let promise = loadSyncTime(&key);
        let future = wasm_bindgen_futures::JsFuture::from(promise);
        let jsvalue = future.await.unwrap();
        let timestamp: f32 = serde_wasm_bindgen::from_value(jsvalue).unwrap();
        Duration::from_secs_f32(timestamp)
    }
}

#[async_trait(?Send)]
impl<T: Item> SpekiProvider<T> for DexieProvider {
    async fn current_time(&self) -> Duration {
        self.time.current_time()
    }

    async fn load_record(&self, id: T::Key) -> Option<Record> {
        let ty = T::identifier();
        let id = JsValue::from_str(&id.to_string());
        let promise = loadRecord(&JsValue::from_str(ty), &id);
        let future = wasm_bindgen_futures::JsFuture::from(promise);
        let jsvalue = future.await.unwrap();
        serde_wasm_bindgen::from_value(jsvalue).unwrap()
    }

    async fn load_all_records(&self) -> HashMap<T::Key, Record> {
        let ty = T::identifier();
        info!("from dexie loading all {ty:?}");
        let promise = loadAllRecords(&JsValue::from_str(ty));
        let future = wasm_bindgen_futures::JsFuture::from(promise);
        let jsvalue = future.await.unwrap();
        let map: HashMap<T::Key, Record> = serde_wasm_bindgen::from_value(jsvalue).unwrap();
        info!("from dexie loaded {:?} {:?}", map.len(), ty);
        map
    }

    async fn save_record_in(&self, space: &str, record: Record) {
        let id = JsValue::from_str(&record.id.to_string());
        let content = JsValue::from_str(&record.content);
        let last_modified = JsValue::from_str(&record.last_modified.to_string());
        saveContent(&JsValue::from_str(space), &id, &content, &last_modified);
    }

    async fn load_ids(&self) -> Vec<T::Key> {
        info!("loaidng all ids of type: {}", T::identifier());
        load_ids::<T>(T::identifier()).await
    }
}


static MICRO: once_cell::sync::Lazy<Arc<AtomicU64>> = once_cell::sync::Lazy::new(|| {
    Arc::new(AtomicU64::new(0))
});


#[async_trait::async_trait(?Send)]
impl<T: Item + Hash + RunLedger<E>, E: LedgerEvent<T> + Serialize + DeserializeOwned + Clone + 'static> LedgerProvider<T, E> for DexieProvider{
    async fn load_ledger(&self) -> Vec<E>{
        let space = format!("{}_ledger", T::identifier());

        let promise = loadAllRecords(&JsValue::from_str(&space));
        let future = wasm_bindgen_futures::JsFuture::from(promise);
        let jsvalue = future.await.unwrap();
        let map: HashMap<String, Record> = serde_wasm_bindgen::from_value(jsvalue).unwrap();

        let mut foo: Vec<LedgerEntry<T, E>> = vec![];

        for (key, value) in map.iter(){
            let key: u64 = key.parse().unwrap();
            let action: E = serde_json::from_str(&value.content).unwrap();
            let timestamp = Duration::from_micros(key + (MICRO.fetch_add(1, Ordering::SeqCst) % 1_000_000));
            let event: LedgerEntry<T, E> = LedgerEntry::new(timestamp, action);
            foo.push(event);
        }

        foo.sort_by_key(|k|k.timestamp);
        foo.into_iter().map(|e| e.event).collect()
    }

    /// Clear the storage area so we can re-run everything.
    async fn reset_space(&self) {
        let space = JsValue::from_str(T::identifier());
        clearSpace(&space);
    }

    async fn reset_ledger(&self) {
        let space = format!("{}_ledger", T::identifier());
        let space = JsValue::from_str(&space);
        clearSpace(&space);
    }


    async fn save_ledger(&self, event: LedgerEntry<T, E>) {
        let space = format!("{}_ledger", T::identifier());
        let id = JsValue::from_str(&event.timestamp.as_micros().to_string());
        let content = JsValue::from_str(&serde_json::to_string(&event.event).unwrap());
        let last_modified = JsValue::from_str(&0.to_string());
        saveContent(&JsValue::from_str(&space), &id, &content, &last_modified);
    }
}

#[wasm_bindgen(module = "/dexie.js")]
extern "C" {
    fn loadDbId() -> Promise;
    fn saveDbId(id: &JsValue);

    fn saveSyncTime(key: &JsValue, lastSync: &JsValue);
    fn loadSyncTime(key: &JsValue) -> Promise;

    fn loadRecord(table: &JsValue, id: &JsValue) -> Promise;
    fn loadAllRecords(table: &JsValue) -> Promise;
    fn saveContent(table: &JsValue, id: &JsValue, content: &JsValue, last_modified: &JsValue);
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

pub async fn load_ids<T: Item>(table: &str) -> Vec<T::Key> {
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
