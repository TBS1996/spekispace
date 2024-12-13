use gloo_utils::format::JsValueSerdeExt;
use js_sys::Promise;
use serde_json::Value;
use std::{path::PathBuf, time::Duration};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/browserfs.js")]
extern "C" {
    fn loadAllFiles(path: &JsValue) -> Promise;
    fn deleteFile(path: &JsValue);
    fn loadFile(path: &JsValue) -> Promise;
    fn saveFile(path: &JsValue, content: &JsValue);
    fn lastModified(path: &JsValue) -> Promise;
    fn loadFilenames(path: &JsValue) -> Promise;
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = Date)]
    fn now() -> f64;
}

pub fn _current_time() -> Duration {
    Duration::from_millis(now() as u64)
}

pub async fn load_filenames(path: &str) -> Vec<String> {
    let path = JsValue::from_str(path);
    let val = promise_to_val(loadFilenames(&path)).await;
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

pub fn delete_file(path: PathBuf) {
    let path = path.to_str().unwrap();
    let path = JsValue::from_str(path);
    deleteFile(&path);
}

pub fn save_file(path: PathBuf, content: &str) {
    let path = path.to_str().unwrap();
    let path = JsValue::from_str(path);
    let content = JsValue::from_str(content);
    saveFile(&path, &content);
}

async fn promise_to_val(promise: Promise) -> Value {
    let future = wasm_bindgen_futures::JsFuture::from(promise);
    let jsvalue = future.await.unwrap();
    jsvalue.into_serde().unwrap()
}

pub async fn last_modified(path: PathBuf) -> Option<Duration> {
    let path = path.to_str().unwrap();
    let path = JsValue::from_str(path);
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

pub async fn load_all_files(path: PathBuf) -> Vec<String> {
    let path = path.to_str().unwrap();
    let path = JsValue::from_str(path);
    let val = promise_to_val(loadAllFiles(&path)).await;
    let arr = val.as_array().unwrap();
    arr.into_iter()
        .map(|elm| match elm {
            Value::String(s) => s.clone(),
            other => panic!("file isnt textfile damn: {}", other),
        })
        .collect()
}

pub async fn load_file(path: PathBuf) -> Option<String> {
    let path = path.to_str().unwrap();
    let path = JsValue::from_str(path);
    let val = promise_to_val(loadFile(&path)).await;

    match val {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        other => panic!("invalid type: {}", other),
    }
}
