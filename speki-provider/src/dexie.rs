use std::{
    collections::{BTreeSet, HashMap},
    time::Duration,
};

use async_trait::async_trait;
use js_sys::Promise;
use serde_json::Value;
use speki_dto::{Item, ProviderId, Record, SpekiProvider, Syncable, TimeProvider};
use tracing::info;
use uuid::Uuid;
use wasm_bindgen::prelude::*;

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

use speki_dto::Indexable;

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

    async fn save_record(&self, record: Record) {
        let ty = T::identifier();
        let id = JsValue::from_str(&record.id.to_string());
        let content = JsValue::from_str(&record.content);
        let last_modified = JsValue::from_str(&record.last_modified.to_string());
        saveContent(&JsValue::from_str(ty), &id, &content, &last_modified);
    }

    async fn load_ids(&self) -> Vec<T::Key> {
        load_ids(T::identifier()).await.into_iter().map(|k|serde_json::from_str(&k.to_string()).unwrap()).collect()
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
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = Date)]
    fn now() -> f64;
}

pub fn _current_time() -> Duration {
    Duration::from_millis(now() as u64)
}

pub async fn load_ids(table: &str) -> Vec<Uuid> {
    let val = promise_to_val(loadAllIds(&JsValue::from_str(table))).await;
    val.as_array()
        .unwrap()
        .into_iter()
        .filter_map(|obj| {
            if let serde_json::Value::String(s) = obj {
                Some(s.parse().unwrap())
            } else {
                None
            }
        })
        .collect()
}

async fn promise_to_val(promise: Promise) -> Value {
    let future = wasm_bindgen_futures::JsFuture::from(promise);
    let jsvalue = future.await.unwrap();
    jsvalue.into_serde().unwrap()
}
