use gloo_utils::format::JsValueSerdeExt;
use js_sys::Promise;
use serde_json::Value;
use std::time::Duration;
use wasm_bindgen::prelude::*;

use crate::log_to_console;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = Date)]
    fn now() -> f64;
}

#[wasm_bindgen(module = "/assets/utils.js")]
extern "C" {
    fn cloneRepo(path: &JsValue, url: &JsValue, token: &JsValue, proxy: &JsValue);
    fn gitClone(path: &JsValue, url: &JsValue, token: &JsValue, proxy: &JsValue);
    fn fetchRepo(path: &JsValue, url: &JsValue, token: &JsValue, proxy: &JsValue);
    fn pullRepo(path: &JsValue, token: &JsValue, proxy: &JsValue);
    fn syncRepo(path: &JsValue, token: &JsValue, proxy: &JsValue);
    fn loadAllFiles(path: &JsValue) -> Promise;
    fn loadRec(path: &JsValue) -> Promise;
    fn newReviews(path: &JsValue) -> Promise;
    fn listFiles(path: &JsValue);
    fn allPaths(path: &JsValue) -> Promise;
    fn deleteFile(path: &JsValue);
    fn loadFile(path: &JsValue) -> Promise;
    fn saveFile(path: &JsValue, content: &JsValue);
    fn validateUpstream(path: &JsValue, token: &JsValue);
    fn deleteDir(path: &JsValue);
    fn lastModified(path: &JsValue) -> Promise;
    fn loadFilenames(path: &JsValue) -> Promise;
}

pub fn current_time() -> Duration {
    Duration::from_millis(now() as u64)
}

pub fn delete_dir(path: &str) {
    let path = JsValue::from_str(path);
    deleteDir(&path);
}

pub async fn load_filenames(path: &str) -> Vec<String> {
    let path = JsValue::from_str(path);
    let val = promise_to_val(loadFilenames(&path)).await;
    log_to_console(&val);
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

pub fn fetch_repo(path: &str, url: &str, token: &str, proxy: &str) {
    let path = JsValue::from_str(path);
    let url = JsValue::from_str(url);
    let token = JsValue::from_str(token);
    let proxy = JsValue::from_str(proxy);
    fetchRepo(&path, &url, &token, &proxy);
}

pub fn clone_repo(path: &str, url: &str, token: &str, proxy: &str) {
    let path = JsValue::from_str(path);
    let url = JsValue::from_str(url);
    let token = JsValue::from_str(token);
    let proxy = JsValue::from_str(proxy);
    cloneRepo(&path, &url, &token, &proxy);
    //gitClone(&path, &url, &token, &proxy);
}

pub fn validate_upstream(path: &str, token: &str) {
    let path = JsValue::from_str(path);
    let token = JsValue::from_str(token);
    validateUpstream(&path, &token);
}

pub fn sync_repo(path: &str, token: &str, proxy: &str) {
    log_to_console("lets sync :D");
    let path = JsValue::from_str(path);
    let token = JsValue::from_str(token);
    let proxy = JsValue::from_str(proxy);
    syncRepo(&path, &token, &proxy);
}

pub fn pull_repo(path: &str, token: &str, proxy: &str) {
    log_to_console("starting pull repo");
    let path = JsValue::from_str(path);
    let token = JsValue::from_str(token);
    let proxy = JsValue::from_str(proxy);
    pullRepo(&path, &token, &proxy);
    log_to_console("rs pull repo ended");
}

pub async fn list_files(path: &str) -> Value {
    let path = JsValue::from_str(path);
    let val = promise_to_val(allPaths(&path)).await;
    val
}

async fn promise_to_val(promise: Promise) -> Value {
    let future = wasm_bindgen_futures::JsFuture::from(promise);
    let jsvalue = future.await.unwrap();
    jsvalue.into_serde().unwrap()
}

pub async fn git_status(path: &str) -> u64 {
    let path = JsValue::from_str(path);
    let val = promise_to_val(newReviews(&path)).await;
    crate::log_to_console(&val);
    match val {
        serde_json::Value::Number(s) => s.as_u64().unwrap(),
        _ => panic!("damn"),
    }
}
