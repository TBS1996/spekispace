use std::time::Duration;
use wasm_bindgen::prelude::*;

use crate::log_to_console;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = Date)]
    fn now() -> f64;
}

#[wasm_bindgen(module = "/assets/cyto.js")]
extern "C" {
    fn createCytoInstance(id: &str);
    fn addNode(cyto_id: &str, id: &str, label: &str, color: &str);
    fn addEdge(cyto_id: &str, source: &str, target: &str);
    fn runLayout(cyto_id: &str);
}

#[wasm_bindgen(module = "/assets/git.js")]
extern "C" {
    fn syncRepo(path: &JsValue, token: &JsValue, proxy: &JsValue);
}

pub fn run_layout(id: &str) {
    runLayout(id);
}

pub fn create_cyto_instance(id: &str) {
    createCytoInstance(id);
}

pub fn add_node(cyto_id: &str, id: &str, label: &str, color: &str) {
    addNode(cyto_id, id, label, color);
}

pub fn add_edge(cyto_id: &str, source: &str, target: &str) {
    addEdge(cyto_id, source, target);
}

pub fn current_time() -> Duration {
    Duration::from_millis(now() as u64)
}

pub fn sync_repo(path: &str, token: &str, proxy: &str) {
    log_to_console("lets sync :D");
    let path = JsValue::from_str(path);
    let token = JsValue::from_str(token);
    let proxy = JsValue::from_str(proxy);
    syncRepo(&path, &token, &proxy);
}
