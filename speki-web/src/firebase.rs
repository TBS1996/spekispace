use std::collections::HashMap;

use gloo_utils::format::JsValueSerdeExt;
use js_sys::Promise;
use serde_json::Value;
use speki_dto::{Cty, Item, ProviderId, Record, SpekiProvider};
use tracing::info;
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

use std::time::Duration;

fn duration_to_firestore_jsvalue(duration: Duration) -> JsValue {
    let obj = js_sys::Object::new();
    js_sys::Reflect::set(
        &obj,
        &JsValue::from("seconds"),
        &JsValue::from_f64(duration.as_secs() as f64),
    )
    .unwrap();
    js_sys::Reflect::set(
        &obj,
        &JsValue::from("nanos"),
        &JsValue::from_f64(duration.subsec_nanos() as f64),
    )
    .unwrap();
    obj.into()
}

use async_trait::async_trait;

#[async_trait(?Send)]
impl<T: Item> SpekiProvider<T> for FirestoreProvider {
    async fn load_record(&self, id: Uuid, ty: Cty) -> Option<Record> {
        let id = JsValue::from_str(&id.to_string());
        let promise = loadRecord(&self.user_id(), &as_js_value(ty), &id);
        let future = wasm_bindgen_futures::JsFuture::from(promise);
        let jsvalue = future.await.unwrap();
        let record: Option<Record> = serde_wasm_bindgen::from_value(jsvalue).unwrap();
        record
    }

    async fn provider_id(&self) -> ProviderId {
        async fn try_load_id(user_id: &JsValue) -> Option<ProviderId> {
            let promise = loadDbId(user_id);
            let future = wasm_bindgen_futures::JsFuture::from(promise);
            let jsvalue = future.await.unwrap();
            serde_wasm_bindgen::from_value::<ProviderId>(jsvalue).ok()
        }

        match try_load_id(&self.user_id()).await {
            Some(id) => id,
            None => {
                let new = ProviderId::new_v4();
                let s = JsValue::from_str(&new.to_string());
                saveDbId(&self.user_id(), &s);
                new
            }
        }
    }

    async fn update_sync(&self, other: ProviderId, ty: Cty, current_time: Duration) {
        let key = format!("{}-{:?}", other, ty);
        let key = JsValue::from_str(&key);
        let val = JsValue::from_f64(current_time.as_secs() as f64);
        saveSyncTime(&self.user_id(), &key, &val);
    }

    async fn last_sync(&self, other: ProviderId, ty: Cty) -> Duration {
        let key = format!("{}-{:?}", other, ty);
        let key = JsValue::from_str(&key);
        let promise = loadSyncTime(&self.user_id(), &key);
        let future = wasm_bindgen_futures::JsFuture::from(promise);
        let jsvalue = future.await.unwrap();
        serde_wasm_bindgen::from_value(jsvalue).unwrap()
    }

    async fn load_all_records(&self, ty: Cty) -> HashMap<Uuid, Record> {
        let promise = loadAllRecords(&self.user_id(), &as_js_value(ty));
        let future = wasm_bindgen_futures::JsFuture::from(promise);
        let jsvalue = future.await.unwrap();
        let records: HashMap<Uuid, Record> = serde_wasm_bindgen::from_value(jsvalue)
            .expect("Failed to deserialize Firestore response");
        records
    }

    async fn save_records(&self, ty: Cty, records: Vec<Record>) {
        info!("from rust starting save-conents");
        use js_sys::{Array, Object};

        let table = JsValue::from_str(as_str(ty));
        let user_id = self.user_id();

        let js_records = Array::new();

        for record in records {
            let js_record = Object::new();

            js_sys::Reflect::set(
                &js_record,
                &JsValue::from("id"),
                &JsValue::from_str(&record.id.to_string()),
            )
            .unwrap();
            js_sys::Reflect::set(
                &js_record,
                &JsValue::from("content"),
                &JsValue::from_str(&record.content),
            )
            .unwrap();
            js_sys::Reflect::set(
                &js_record,
                &JsValue::from("lastModified"),
                &duration_to_firestore_jsvalue(Duration::from_secs(record.last_modified)),
            )
            .unwrap();

            js_records.push(&js_record);
        }

        let js_records_value: JsValue = js_records.into();

        saveContents(&user_id, &table, &js_records_value);
    }

    async fn save_record(&self, ty: Cty, record: Record) {
        let table = JsValue::from_str(as_str(ty));
        let content_id = JsValue::from_str(&record.id.to_string());
        let content = JsValue::from_str(&record.content);
        let last_modified =
            duration_to_firestore_jsvalue(Duration::from_secs(record.last_modified));

        saveContent(
            &self.user_id(),
            &table,
            &content_id,
            &content,
            &last_modified,
        );
    }
}

#[wasm_bindgen(module = "/assets/firebase.js")]
extern "C" {
    fn saveContent(
        user_id: &JsValue,
        table: &JsValue,
        content_id: &JsValue,
        content: &JsValue,
        last_modified: &JsValue,
    );

    fn saveContents(user_id: &JsValue, table: &JsValue, contents: &JsValue);
    fn deleteContent(user_id: &JsValue, table: &JsValue, id: &JsValue);
    fn loadRecord(user_id: &JsValue, table: &JsValue, id: &JsValue) -> Promise;
    fn loadAllRecords(user_id: &JsValue, table: &JsValue) -> Promise;
    fn loadAllIds(user_id: &JsValue, table: &JsValue) -> Promise;
    fn lastModified(user_id: &JsValue, table: &JsValue, id: &JsValue) -> Promise;

    fn loadDbId(user_id: &JsValue) -> Promise;
    fn saveDbId(user_id: &JsValue, id: &JsValue); // todo: generate it server side

    fn saveSyncTime(user_id: &JsValue, key: &JsValue, lastSync: &JsValue);
    fn loadSyncTime(user_id: &JsValue, key: &JsValue) -> Promise;

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
