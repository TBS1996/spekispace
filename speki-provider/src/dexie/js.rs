use gloo_utils::format::JsValueSerdeExt;
use js_sys::Promise;
use serde_json::Value;
use std::time::Duration;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/dexie.js")]
extern "C" {
    fn saveFile(id: &JsValue, content: &JsValue);
    fn loadFile(id: &JsValue) -> Promise;
    fn saveReviews(id: &JsValue, content: &JsValue);
    fn loadReviews(id: &JsValue) -> Promise;
    fn deleteFile(id: &JsValue);
    fn loadAllFiles() -> Promise;
    fn lastModified(id: &JsValue) -> Promise;
    fn loadIds() -> Promise;
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = Date)]
    fn now() -> f64;
}

pub fn _current_time() -> Duration {
    Duration::from_millis(now() as u64)
}

pub async fn load_ids() -> Vec<String> {
    let val = promise_to_val(loadIds()).await;
    val.as_array()
        .unwrap()
        .into_iter()
        .filter_map(|obj| {
            if let serde_json::Value::String(s) = obj {
                Some(s.clone())
            } else {
                None
            }
        })
        .collect()
}

pub fn delete_file(id: &str) {
    let path = JsValue::from_str(id);
    deleteFile(&path);
}

pub fn save_reviews(id: &str, content: &str) {
    let path = JsValue::from_str(id);
    let content = JsValue::from_str(content);
    saveReviews(&path, &content);
}

pub fn save_file(id: &str, content: &str) {
    let path = JsValue::from_str(id);
    let content = JsValue::from_str(content);
    saveFile(&path, &content);
}

async fn promise_to_val(promise: Promise) -> Value {
    let future = wasm_bindgen_futures::JsFuture::from(promise);
    let jsvalue = future.await.unwrap();
    jsvalue.into_serde().unwrap()
}

pub async fn last_modified(id: &str) -> Option<Duration> {
    let path = JsValue::from_str(id);
    let val = promise_to_val(lastModified(&path)).await;
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

pub async fn load_all_files() -> Vec<String> {
    let val = promise_to_val(loadAllFiles()).await;
    let arr = val.as_array().unwrap();
    arr.into_iter()
        .map(|elm| match elm {
            Value::String(s) => s.clone(),
            other => panic!("file isnt textfile damn: {}", other),
        })
        .collect()
}

pub async fn load_file(id: &str) -> Option<String> {
    let path = JsValue::from_str(id);
    let val = promise_to_val(loadFile(&path)).await;

    match val {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        other => panic!("invalid type: {}", other),
    }
}

pub async fn load_reviews(id: &str) -> Option<String> {
    let path = JsValue::from_str(id);
    let val = promise_to_val(loadReviews(&path)).await;

    match val {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        other => panic!("invalid type: {}", other),
    }
}
