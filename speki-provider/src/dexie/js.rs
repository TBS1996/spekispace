use gloo_utils::format::JsValueSerdeExt;
use js_sys::Promise;
use serde_json::Value;
use std::time::Duration;
use uuid::Uuid;
use wasm_bindgen::prelude::*;

use crate::dexie::Table;

#[wasm_bindgen(module = "/dexie.js")]
extern "C" {
    fn saveContent(table: &JsValue, id: &JsValue, content: &JsValue);
    fn deleteContent(table: &JsValue, id: &JsValue);
    fn loadContent(table: &JsValue, id: &JsValue) -> Promise;
    fn loadAllContent(table: &JsValue) -> Promise;
    fn loadAllIds(table: &JsValue) -> Promise;
    fn lastModified(table: &JsValue, id: &JsValue) -> Promise;
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = Date)]
    fn now() -> f64;
}

pub fn _current_time() -> Duration {
    Duration::from_millis(now() as u64)
}

pub async fn load_ids(table: Table) -> Vec<Uuid> {
    let val = promise_to_val(loadAllIds(&table.as_js_value())).await;
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

pub fn delete_file(table: Table, id: &str) {
    let id = JsValue::from_str(id);
    deleteContent(&table.as_js_value(), &id);
}

pub fn save_content(table: Table, id: &str, content: &str) {
    let id = JsValue::from_str(id);
    let content = JsValue::from_str(content);
    saveContent(&table.as_js_value(), &id, &content);
}

async fn promise_to_val(promise: Promise) -> Value {
    let future = wasm_bindgen_futures::JsFuture::from(promise);
    let jsvalue = future.await.unwrap();
    jsvalue.into_serde().unwrap()
}

pub async fn last_modified(table: Table, id: &str) -> Option<Duration> {
    let path = JsValue::from_str(id);
    let val = promise_to_val(lastModified(&table.as_js_value(), &path)).await;
    let serde_json::Value::String(s) = val else {
        return None;
    };

    let datetime =
        time::OffsetDateTime::parse(&s, &time::format_description::well_known::Rfc3339).unwrap();
    let unix_epoch = time::OffsetDateTime::UNIX_EPOCH;
    let duration_since_epoch = datetime - unix_epoch;
    let seconds = duration_since_epoch.whole_seconds();
    Some(Duration::from_secs(seconds as u64))
}

pub async fn load_all_files(table: Table) -> Vec<String> {
    let val = promise_to_val(loadAllContent(&table.as_js_value())).await;
    let arr = val.as_array().unwrap();
    arr.into_iter()
        .map(|elm| match elm {
            Value::String(s) => s.clone(),
            other => panic!("file isnt textfile damn: {}", other),
        })
        .collect()
}

pub async fn load_content(table: Table, id: &str) -> Option<String> {
    let path = JsValue::from_str(id);
    let val = promise_to_val(loadContent(&table.as_js_value(), &path)).await;

    match val {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        other => panic!("invalid type: {}", other),
    }
}
