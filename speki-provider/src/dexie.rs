use std::{collections::HashMap, time::Duration};

use gloo_utils::format::JsValueSerdeExt;
use js_sys::Promise;
use serde_json::Value;
use speki_dto::{CardId, Record, SpekiProvider};
use uuid::Uuid;
use wasm_bindgen::prelude::*;

use speki_dto::{Config, Cty};

pub struct DexieProvider;

use async_trait::async_trait;

#[async_trait(?Send)]
impl SpekiProvider for DexieProvider {
    async fn load_record(&self, id: Uuid, ty: Cty) -> Option<Record> {
        let id = JsValue::from_str(&id.to_string());
        let promise = loadRecord(&cty_as_jsvalue(ty), &id);
        let future = wasm_bindgen_futures::JsFuture::from(promise);
        let jsvalue = future.await.unwrap();
        serde_wasm_bindgen::from_value(jsvalue).unwrap()
    }

    async fn load_all_records(&self, ty: Cty) -> HashMap<Uuid, Record> {
        let promise = loadAllRecords(&cty_as_jsvalue(ty));
        let future = wasm_bindgen_futures::JsFuture::from(promise);
        let jsvalue = future.await.unwrap();
        serde_wasm_bindgen::from_value(jsvalue).unwrap()
    }

    async fn delete_content(&self, id: Uuid, ty: Cty) {
        let id = JsValue::from_str(&id.to_string());
        deleteContent(&cty_as_jsvalue(ty), &id);
    }

    async fn save_content(&self, ty: Cty, record: Record) {
        let id = JsValue::from_str(&record.id.to_string());
        let content = JsValue::from_str(&record.content);
        let last_modified = JsValue::from_str(&record.last_modified.to_string());
        saveContent(&cty_as_jsvalue(ty), &id, &content, &last_modified);
    }

    async fn load_card_ids(&self) -> Vec<CardId> {
        load_ids(Cty::Card).await.into_iter().collect()
    }

    async fn load_config(&self) -> Config {
        Config
    }

    async fn save_config(&self, _config: Config) {
        todo!()
    }
}

#[wasm_bindgen(module = "/dexie.js")]
extern "C" {
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
