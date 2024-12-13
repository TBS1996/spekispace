use gloo_utils::format::JsValueSerdeExt;
use js_sys::Promise;
use serde_json::Value;
use std::time::Duration;
use uuid::Uuid;
use wasm_bindgen::prelude::*;

use crate::dexie::Cty;

#[wasm_bindgen(module = "/dexie.js")]
extern "C" {
    fn saveContent(table: &JsValue, id: &JsValue, content: &JsValue);
    fn deleteContent(table: &JsValue, id: &JsValue);
    fn loadContent(table: &JsValue, id: &JsValue) -> Promise;
    fn loadAllContent(table: &JsValue) -> Promise;
    fn loadAllIds(table: &JsValue) -> Promise;
    fn lastModified(table: &JsValue, id: &JsValue) -> Promise;
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

pub async fn last_modified(table: Cty, id: &str) -> Option<Duration> {
    let path = JsValue::from_str(id);
    let val = promise_to_val(lastModified(&cty_as_jsvalue(table), &path)).await;
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

pub async fn load_all_files(table: Cty) -> Vec<String> {
    let val = promise_to_val(loadAllContent(&cty_as_jsvalue(table))).await;
    let arr = val.as_array().unwrap();
    arr.into_iter()
        .map(|elm| match elm {
            Value::String(s) => s.clone(),
            other => panic!("file isnt textfile damn: {}", other),
        })
        .collect()
}

pub async fn load_content(table: Cty, id: &str) -> Option<String> {
    let path = JsValue::from_str(id);
    let val = promise_to_val(loadContent(&cty_as_jsvalue(table), &path)).await;

    match val {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        other => panic!("invalid type: {}", other),
    }
}
