use std::{collections::HashMap, time::Duration};

use gloo_utils::format::JsValueSerdeExt;
use js_sys::Promise;
use serde_json::Value;
use speki_dto::{CardId, ProviderId, Record, SpekiProvider};
use tracing::info;
use uuid::Uuid;
use wasm_bindgen::prelude::*;

use speki_dto::Cty;

pub struct DexieProvider;

use async_trait::async_trait;
use speki_dto::Item;

#[async_trait(?Send)]
impl<T: Item> SpekiProvider<T> for DexieProvider {
    async fn load_record(&self, id: Uuid) -> Option<Record> {
        let ty = T::identifier();
        let id = JsValue::from_str(&id.to_string());
        let promise = loadRecord(&cty_as_jsvalue(ty), &id);
        let future = wasm_bindgen_futures::JsFuture::from(promise);
        let jsvalue = future.await.unwrap();
        serde_wasm_bindgen::from_value(jsvalue).unwrap()
    }

    async fn provider_id(&self) -> ProviderId {
        info!("getting provider id from dexie");
        async fn try_load_id() -> Option<ProviderId> {
            let promise = loadDbId();
            let future = wasm_bindgen_futures::JsFuture::from(promise);
            let jsvalue = future.await.unwrap();
            serde_wasm_bindgen::from_value::<ProviderId>(jsvalue).ok()
        }

        match try_load_id().await {
            Some(id) => {
                info!("found dexie id: {:?}", id);
                id
            }
            None => {
                info!("creating new dexie db id");
                let new = ProviderId::new_v4();
                let s = JsValue::from_str(&new.to_string());
                saveDbId(&s);
                new
            }
        }
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

    async fn load_all_records(&self) -> HashMap<Uuid, Record> {
        let ty = T::identifier();
        let promise = loadAllRecords(&cty_as_jsvalue(ty));
        let future = wasm_bindgen_futures::JsFuture::from(promise);
        let jsvalue = future.await.unwrap();
        serde_wasm_bindgen::from_value(jsvalue).unwrap()
    }

    async fn save_record(&self, record: Record) {
        let ty = T::identifier();
        let id = JsValue::from_str(&record.id.to_string());
        let content = JsValue::from_str(&record.content);
        let last_modified = JsValue::from_str(&record.last_modified.to_string());
        saveContent(&cty_as_jsvalue(ty), &id, &content, &last_modified);
    }

    async fn load_ids(&self) -> Vec<CardId> {
        load_ids(Cty::Card).await.into_iter().collect()
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

fn cty_as_jsvalue(ty: Cty) -> JsValue {
    let name = match ty {
        Cty::Attribute => "attrs",
        Cty::Card => "cards",
        Cty::Review => "reviews",
    };

    JsValue::from_str(name)
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = Date)]
    fn now() -> f64;
}

pub fn _current_time() -> Duration {
    Duration::from_millis(now() as u64)
}

pub async fn load_ids(table: Cty) -> Vec<Uuid> {
    let val = promise_to_val(loadAllIds(&cty_as_jsvalue(table))).await;
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
