use std::{collections::HashMap, time::Duration};

use async_trait::async_trait;
use gloo_utils::format::JsValueSerdeExt;
use js_sys::Promise;
use serde_json::Value;
use speki_dto::{Cty, Item, ProviderId, Record, SpekiProvider, Syncable, TimeProvider};
use speki_provider::WasmTime;
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
    time: WasmTime,
}

impl FirestoreProvider {
    pub fn new(user: AuthUser) -> Self {
        Self {
            user_id: user.uid,
            time: WasmTime,
        }
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

#[async_trait(?Send)]
impl<T: Item> Syncable<T> for FirestoreProvider {
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

    async fn load_all_after(&self, not_before: Duration) -> HashMap<Uuid, T> {
        let ty = T::identifier();
        let not_before = JsValue::from_f64(not_before.as_secs_f64());
        let promise = loadAllRecords(&self.user_id(), &as_js_value(ty), &not_before);
        let future = wasm_bindgen_futures::JsFuture::from(promise);
        let jsvalue = future.await.unwrap();
        let records: HashMap<Uuid, Record> = serde_wasm_bindgen::from_value(jsvalue).unwrap();

        let mut outmap = HashMap::default();

        for (key, val) in records.into_iter() {
            let item: T = <T as Item>::deserialize(key, val.content);
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

    async fn load_record(&self, id: Uuid) -> Option<Record> {
        let ty = T::identifier();
        let id = JsValue::from_str(&id.to_string());
        let promise = loadRecord(&self.user_id(), &as_js_value(ty), &id);
        let future = wasm_bindgen_futures::JsFuture::from(promise);
        let jsvalue = future.await.unwrap();
        let record: Option<Record> = serde_wasm_bindgen::from_value(jsvalue).unwrap();
        record
    }

    async fn load_all_records(&self) -> HashMap<Uuid, Record> {
        let ty = T::identifier();
        let not_before = JsValue::from_f64(Duration::default().as_secs_f64());
        let promise = loadAllRecords(&self.user_id(), &as_js_value(ty), &not_before);
        let future = wasm_bindgen_futures::JsFuture::from(promise);
        let jsvalue = future.await.unwrap();
        let records: HashMap<Uuid, Record> = serde_wasm_bindgen::from_value(jsvalue).unwrap();
        records
    }

    async fn save_records(&self, records: Vec<Record>) {
        info!("from rust starting save-records");
        use js_sys::{Array, Object};
        let ty = T::identifier();

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

    async fn save_record(&self, record: Record) {
        SpekiProvider::<T>::save_records(self, vec![record]).await;
    }
}

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
