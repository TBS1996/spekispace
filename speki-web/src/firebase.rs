use std::{collections::HashMap, time::Duration};
use async_trait::async_trait;
#[cfg(feature = "web")]
use js_sys::Promise;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use speki_dto::ProviderId;
use tracing::info;
#[cfg(feature = "web")]
use wasm_bindgen::prelude::*;

use speki_provider::WasmTime;


use crate::LOGIN_STATE;

#[derive(Clone)]
pub struct FirestoreProvider {
    user_id: String,
    time: WasmTime,
    id: Option<ProviderId>,
}

#[cfg(feature = "web")]
impl FirestoreProvider {
    pub fn new(user: AuthUser) -> Self {
        Self {
            user_id: user.uid,
            time: WasmTime,
            id: None,
        }
    }

    pub fn set_id(&mut self, id: ProviderId) {
        self.id = Some(id);
    }

    fn user_id(&self) -> JsValue {
        JsValue::from_str(&self.user_id)
    }
}

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

/* 
#[async_trait(?Send)]
impl<T: Item> Syncable<T> for FirestoreProvider {
    async fn save_id(&self, id: ProviderId) {
        let s = JsValue::from_str(&id.to_string());
        saveDbId(&self.user_id(), &s);
    }

    async fn load_id_opt(&self) -> Option<ProviderId> {
        if self.id.is_some() {
            return self.id;
        }

        let promise = loadDbId(&self.user_id());
        let future = wasm_bindgen_futures::JsFuture::from(promise);
        let jsvalue = future.await.unwrap();
        serde_wasm_bindgen::from_value::<ProviderId>(jsvalue).ok()
    }

    async fn update_sync_info(&self, other: ProviderId, current_time: Duration) {
        let ty = T::identifier();
        let key = format!("{}-{:?}", other, ty);
        let key = JsValue::from_str(&key);
        let val = JsValue::from_f64(current_time.as_secs() as f64);
        saveSyncTime(&self.user_id(), &key, &val);
    }

    async fn last_sync(&self, other: ProviderId) -> Duration {
        let ty = T::identifier();
        let key = format!("{}-{:?}", other, ty);
        let key = JsValue::from_str(&key);
        let promise = loadSyncTime(&self.user_id(), &key);
        let future = wasm_bindgen_futures::JsFuture::from(promise);
        let jsvalue = future.await.unwrap();
        let timestamp: f32 = serde_wasm_bindgen::from_value(jsvalue).unwrap();
        Duration::from_secs_f32(timestamp)
    }

    async fn load_all_after(&self, not_before: Duration) -> HashMap<T::Key, T> {
        let ty = T::identifier();
        let not_before = JsValue::from_f64(not_before.as_secs_f64());
        let promise = loadAllRecords(&self.user_id(), &JsValue::from_str(ty), &not_before);
        let future = wasm_bindgen_futures::JsFuture::from(promise);
        let jsvalue = future.await.unwrap();
        let records: HashMap<T::Key, Record> = serde_wasm_bindgen::from_value(jsvalue).unwrap();

        let mut outmap = HashMap::default();

        for (key, val) in records.into_iter() {
            let item: T = <T as Item>::item_deserialize(val.content);
            outmap.insert(key, item);
        }

        outmap
    }
}

#[async_trait(?Send)]
impl<T: Item> SpekiProvider<T> for FirestoreProvider {
    async fn current_time(&self) -> Duration {
        self.time.current_time()
    }

    async fn load_content(&self, id: T::Key) -> Option<Record> {
        let ty = T::identifier();
        let id = JsValue::from_str(&id.to_string());
        let promise = loadRecord(&self.user_id(), &JsValue::from_str(ty), &id);
        let future = wasm_bindgen_futures::JsFuture::from(promise);
        let jsvalue = future.await.unwrap();
        let record: Option<Record> = serde_wasm_bindgen::from_value(jsvalue).unwrap();
        record
    }

    async fn load_all_contents(&self) -> HashMap<T::Key, Record> {
        let ty = T::identifier();
        let not_before = JsValue::from_f64(Duration::default().as_secs_f64());
        let promise = loadAllRecords(&self.user_id(), &JsValue::from_str(ty), &not_before);
        let future = wasm_bindgen_futures::JsFuture::from(promise);
        let jsvalue = future.await.unwrap();
        let records: HashMap<T::Key, Record> = serde_wasm_bindgen::from_value(jsvalue).unwrap();
        records
    }

    async fn save_records(&self, records: Vec<Record>) {
        info!("from rust starting save-records");
        use js_sys::{Array, Object};
        let ty = T::identifier();

        let table = JsValue::from_str(ty);
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
            js_sys::Reflect::set(
                &js_record,
                &JsValue::from("inserted"),
                &JsValue::from_f64(record.inserted.unwrap() as f64),
            )
            .unwrap();

            js_records.push(&js_record);
        }

        let js_records_value: JsValue = js_records.into();

        saveContents(&user_id, &table, &js_records_value);
    }

    async fn save_content(&self, space: &str, record: Record) {
        SpekiProvider::<T>::save_records(self, vec![record]).await;
    }
}
*/

#[wasm_bindgen(module = "/assets/firebase.js")]
extern "C" {
    fn saveContents(user_id: &JsValue, table: &JsValue, contents: &JsValue);
    fn deleteContent(user_id: &JsValue, table: &JsValue, id: &JsValue);
    fn loadRecord(user_id: &JsValue, table: &JsValue, id: &JsValue) -> Promise;
    fn loadAllRecords(user_id: &JsValue, table: &JsValue, not_before: &JsValue) -> Promise;
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

async fn try_promise_to_val(promise: Promise) -> Option<Value> {
    info!("lets goo");
    let future = wasm_bindgen_futures::JsFuture::from(promise);
    info!("future!");
    let jsvalue = future.await.ok()?;
    info!("whoa!");
    jsvalue.into_serde().ok()
}

async fn promise_to_val(promise: Promise) -> Value {
    try_promise_to_val(promise).await.unwrap()
}

pub async fn current_sign_in() -> Option<AuthUser> {
    let val = try_promise_to_val(getCurrentUser()).await?;
    info!("curr user {val:?}");

    if val.is_object() {
        Some(serde_json::from_value(val).unwrap())
    } else {
        None
    }
}

pub async fn sign_out() {
    let _ = promise_to_val(signOutUser()).await;
    LOGIN_STATE.write().take();
}

pub async fn sign_in() -> Option<AuthUser> {
    if let Some(user) = current_sign_in().await {
        return Some(user);
    }

    let val = promise_to_val(signInWithGoogle()).await;
    let user: AuthUser = serde_json::from_value(val).unwrap();

    *LOGIN_STATE.write() = Some(user.clone());
    Some(user)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthUser {
    pub api_key: String,
    pub app_name: String,
    pub created_at: String,
    pub email_verified: bool,
    pub is_anonymous: bool,
    pub last_login_at: String,
    pub uid: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub photo_url: Option<String>,
    pub provider_data: Vec<serde_json::Value>,
    pub sts_token_manager: Option<serde_json::Value>,
}

impl AuthUser {
    pub fn display(&self) -> String {
        if let Some(name) = self.display_name.clone() {
            name
        } else if let Some(email) = self.email.clone() {
            email
        } else {
            self.uid.clone()
        }
    }
}
