use std::{collections::HashMap, time::Duration};

use gloo_utils::format::JsValueSerdeExt;
use js_sys::Promise;
use serde::Deserialize;
use serde_json::Value;
use speki_dto::{Config, Cty, SpekiProvider};
use uuid::Uuid;
use wasm_bindgen::prelude::*;

fn as_str(ty: Cty) -> &'static str {
    match ty {
        Cty::Card => "cards",
        Cty::Review => "reviews",
        Cty::Attribute => "attributes",
    }
}

fn as_js_value(ty: Cty) -> JsValue {
    JsValue::from_str(as_str(ty))
}

pub struct FirestoreProvider {
    user_id: String,
}

impl FirestoreProvider {
    pub fn new(user: AuthUser) -> Self {
        Self { user_id: user.uid }
    }
    fn user_id(&self) -> JsValue {
        JsValue::from_str(&self.user_id)
    }
}

use async_trait::async_trait;

#[async_trait(?Send)]
impl SpekiProvider for FirestoreProvider {
    async fn load_content(&self, id: Uuid, ty: Cty) -> Option<String> {
        let content_id = JsValue::from_str(&id.to_string());
        let promise = loadContent(&self.user_id(), &as_js_value(ty), &content_id);
        match promise_to_val(promise).await {
            Value::Null => None,
            Value::String(s) => Some(s),
            _ => panic!("damn wth"),
        }
    }
    async fn last_modified(&self, id: Uuid, ty: Cty) -> Option<Duration> {
        let content_id = JsValue::from_str(&id.to_string());
        let val =
            promise_to_val(lastModified(&self.user_id(), &as_js_value(ty), &content_id)).await;
        let serde_json::Value::String(s) = val else {
            return None;
        };

        let datetime =
            time::OffsetDateTime::parse(&s, &time::format_description::well_known::Rfc3339)
                .unwrap();
        let unix_epoch = time::OffsetDateTime::UNIX_EPOCH;
        let duration_since_epoch = datetime - unix_epoch;
        let seconds = duration_since_epoch.whole_seconds();
        Some(Duration::from_secs(seconds as u64))
    }
    async fn load_all_content(&self, ty: Cty) -> Vec<String> {
        // let wtf = load_all_records(self.user_id.parse().unwrap(), ty.clone()).await;
        //tracing::info!("wtF: {:?}", wtf);
        let promise = loadAllContent(&self.user_id(), &as_js_value(ty));
        let val = promise_to_val(promise).await;
        val.as_array()
            .unwrap()
            .into_iter()
            .map(|val| match val {
                Value::String(s) => s.clone(),
                x => {
                    tracing::info!("err lol: {:?}", x);
                    panic!();
                }
            })
            .collect()
    }
    async fn save_content(&self, ty: Cty, id: Uuid, content: String) {
        let table = JsValue::from_str(as_str(ty));
        let content_id = JsValue::from_str(&id.to_string());
        let content = JsValue::from_str(&content);
        saveContent(&self.user_id(), &table, &content_id, &content);
    }
    async fn delete_content(&self, id: Uuid, ty: Cty) {
        let content_id = JsValue::from_str(&id.to_string());
        deleteContent(&self.user_id(), &as_js_value(ty), &content_id);
    }

    async fn load_config(&self) -> Config {
        Config
    }

    async fn save_config(&self, _config: Config) {
        todo!()
    }
}

#[wasm_bindgen(module = "/assets/firebase.js")]
extern "C" {
    fn saveContent(user_id: &JsValue, table: &JsValue, content_id: &JsValue, content: &JsValue);
    fn deleteContent(user_id: &JsValue, table: &JsValue, id: &JsValue);
    fn loadContent(user_id: &JsValue, table: &JsValue, id: &JsValue) -> Promise;
    fn loadAllContent(user_id: &JsValue, table: &JsValue) -> Promise;
    fn loadAllIds(user_id: &JsValue, table: &JsValue) -> Promise;
    fn lastModified(user_id: &JsValue, table: &JsValue, id: &JsValue) -> Promise;
    fn loadAll(user_id: &JsValue, table: &JsValue) -> Promise;

    fn signInWithGoogle() -> Promise;
    fn signOutUser() -> Promise;
    fn getCurrentUser() -> Promise;
    fn isUserAuthenticated() -> Promise;
}

async fn promise_to_val(promise: Promise) -> Value {
    let future = wasm_bindgen_futures::JsFuture::from(promise);
    let jsvalue = future.await.unwrap();
    jsvalue.into_serde().unwrap()
}

pub async fn sign_in() -> AuthUser {
    let val = promise_to_val(signInWithGoogle()).await;
    AuthUser::try_from(val).unwrap()
}

impl TryFrom<Value> for AuthUser {
    type Error = ();

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let obj = value.as_object().unwrap();
        let uid = obj.get("uid").unwrap().as_str().unwrap().to_owned();

        Ok(Self { uid })
    }
}

#[derive(Default, Clone, Debug)]
pub struct AuthUser {
    pub uid: String,
}

#[derive(Debug, Deserialize)]
struct Record {
    content: String,
    last_modified: Duration,
}

async fn load_all_records(user_id: Uuid, ty: Cty) -> HashMap<Uuid, Record> {
    let user_id = JsValue::from_str(&user_id.to_string());
    let promise = loadAll(&user_id, &as_js_value(ty));
    let future = wasm_bindgen_futures::JsFuture::from(promise);
    let jsvalue = future.await.unwrap();
    let records: HashMap<Uuid, Record> =
        serde_wasm_bindgen::from_value(jsvalue).expect("Failed to deserialize Firestore response");
    records
}
