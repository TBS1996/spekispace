use gloo_utils::format::JsValueSerdeExt;
use js_sys::Promise;
use serde_json::Value;
use speki_dto::Record;
use std::{collections::HashMap, time::Duration};
use uuid::Uuid;
use wasm_bindgen::prelude::*;

use crate::dexie::Cty;

#[wasm_bindgen(module = "/dexie.js")]
extern "C" {
    fn loadRecord(table: &JsValue, id: &JsValue) -> Promise;
    fn loadAllRecords(table: &JsValue) -> Promise;

    fn saveContent(table: &JsValue, id: &JsValue, content: &JsValue);
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

pub fn delete_file(table: Cty, id: &str) {
    let id = JsValue::from_str(id);
    deleteContent(&cty_as_jsvalue(table), &id);
}

pub fn save_content(table: Cty, id: &str, content: &str) {
    let id = JsValue::from_str(id);
    let content = JsValue::from_str(content);
    saveContent(&cty_as_jsvalue(table), &id, &content);
}

async fn promise_to_val(promise: Promise) -> Value {
    let future = wasm_bindgen_futures::JsFuture::from(promise);
    let jsvalue = future.await.unwrap();
    jsvalue.into_serde().unwrap()
}

pub async fn load_record(id: Uuid, ty: Cty) -> Option<Record> {
    let id = JsValue::from_str(&id.to_string());
    let promise = loadRecord(&cty_as_jsvalue(ty), &id);
    let future = wasm_bindgen_futures::JsFuture::from(promise);
    let jsvalue = future.await.unwrap();
    let record: Option<Record> = serde_wasm_bindgen::from_value(jsvalue).unwrap();
    record
}

pub async fn load_all_records(ty: Cty) -> HashMap<Uuid, Record> {
    let promise = loadAllRecords(&cty_as_jsvalue(ty));
    let future = wasm_bindgen_futures::JsFuture::from(promise);
    let jsvalue = future.await.unwrap();
    let record: HashMap<Uuid, Record> = serde_wasm_bindgen::from_value(jsvalue).unwrap();
    record
}
