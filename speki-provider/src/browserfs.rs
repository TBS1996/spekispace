use std::{collections::HashMap, path::PathBuf, time::Duration};

use async_trait::async_trait;
use speki_dto::{CardId, Cty, ProviderId, ProviderMeta, Record, SpekiProvider};
use uuid::Uuid;

mod js {
    use std::{path::PathBuf, time::Duration};

    use gloo_utils::format::JsValueSerdeExt;
    use js_sys::Promise;
    use serde_json::Value;
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
}

pub struct BrowserFsProvider {
    repo: PathBuf,
}

impl BrowserFsProvider {
    pub fn new(path: &str) -> Self {
        Self {
            repo: PathBuf::from(path),
        }
    }

    fn review_path(&self) -> PathBuf {
        self.repo.join("reviews")
    }

    fn attrs_path(&self) -> PathBuf {
        self.repo.join("attributes")
    }

    fn cards_path(&self) -> PathBuf {
        self.repo.join("cards")
    }

    fn folder_path(&self, ty: Cty) -> PathBuf {
        match ty {
            Cty::Attribute => self.attrs_path(),
            Cty::Review => self.review_path(),
            Cty::Card => self.cards_path(),
        }
    }

    fn content_path(&self, ty: Cty, id: Uuid) -> PathBuf {
        self.folder_path(ty).join(id.to_string())
    }
}

use speki_dto::Item;

#[async_trait(?Send)]
impl<T: Clone + Item + 'static> SpekiProvider<T> for BrowserFsProvider {
    async fn load_record(&self, _id: Uuid, _ty: Cty) -> Option<Record> {
        todo!()
    }

    async fn provider_id(&self) -> ProviderId {
        todo!()
    }

    async fn update_sync_info(&self, other: ProviderId, ty: Cty, current_time: Duration) {
        todo!()
    }

    async fn last_sync(&self, other: ProviderId, ty: Cty) -> Duration {
        todo!()
    }

    async fn load_all_records(&self, _ty: Cty) -> HashMap<Uuid, Record> {
        todo!()
    }

    async fn save_record(&self, ty: Cty, record: Record) {
        js::save_file(
            self.content_path(ty, record.id.parse().unwrap()),
            &record.content,
        );
    }

    async fn load_ids(&self) -> Vec<CardId> {
        js::load_filenames(self.folder_path(Cty::Card).to_str().unwrap())
            .await
            .into_iter()
            .map(|id| id.parse().unwrap())
            .collect()
    }
}
