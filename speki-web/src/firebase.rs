use gloo_utils::format::JsValueSerdeExt;
use js_sys::Promise;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::{str::FromStr, time::Duration};
use tracing::info;
use uuid::Uuid;
use wasm_bindgen::prelude::*;

use speki_dto::{
    AttributeDTO, AttributeId, CardId, Config, RawCard, Recall, Review, SpekiProvider,
};

enum Table {
    Cards,
    Reviews,
    Attributes,
}

impl Table {
    fn as_str(&self) -> &'static str {
        match self {
            Table::Cards => "cards",
            Table::Reviews => "reviews",
            Table::Attributes => "attributes",
        }
    }

    fn as_js_value(&self) -> JsValue {
        JsValue::from_str(self.as_str())
    }
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

    async fn last_modified(&self, table: Table, content_id: Uuid) -> Option<Duration> {
        let content_id = JsValue::from_str(&content_id.to_string());
        let val = promise_to_val(lastModified(
            &self.user_id(),
            &table.as_js_value(),
            &content_id,
        ))
        .await;
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

    async fn raw_load_content(&self, table: Table, content_id: Uuid) -> Option<String> {
        let content_id = JsValue::from_str(&content_id.to_string());
        let promise = loadContent(&self.user_id(), &table.as_js_value(), &content_id);
        match promise_to_val(promise).await {
            Value::Null => None,
            Value::String(s) => Some(s),
            _ => panic!("damn wth"),
        }
    }

    async fn load_content<T: DeserializeOwned>(&self, table: Table, content_id: Uuid) -> Option<T> {
        self.raw_load_content(table, content_id)
            .await
            .map(|s| toml::from_str(&s).unwrap())
    }

    async fn load_ids(&self, table: Table) -> Vec<Uuid> {
        info!("fire loading id");
        let val = promise_to_val(loadAllIds(&self.user_id(), &table.as_js_value())).await;
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

    async fn load_all_content<T: DeserializeOwned>(&self, table: Table) -> Vec<T> {
        let promise = loadAllContent(&self.user_id(), &table.as_js_value());
        let val = promise_to_val(promise).await;
        val.as_array()
            .unwrap()
            .into_iter()
            .map(|val| match val {
                Value::String(s) => toml::from_str(s).unwrap(),
                _ => panic!(),
            })
            .collect()
    }

    fn raw_save_content(&self, table: Table, content_id: Uuid, content: String) {
        let table = JsValue::from_str(table.as_str());
        let content_id = JsValue::from_str(&content_id.to_string());
        let content = JsValue::from_str(&content);
        saveContent(&self.user_id(), &table, &content_id, &content);
    }

    fn save_content<T: Serialize>(&self, table: Table, content_id: Uuid, content: T) {
        let content: String = toml::to_string(&content).unwrap();
        self.raw_save_content(table, content_id, content);
    }

    fn delete_content(&self, table: Table, content_id: Uuid) {
        let content_id = JsValue::from_str(&content_id.to_string());
        deleteContent(&self.user_id(), &table.as_js_value(), &content_id);
    }
}

use async_trait::async_trait;

#[async_trait(?Send)]
impl SpekiProvider for FirestoreProvider {
    async fn load_card_ids(&self) -> Vec<CardId> {
        self.load_ids(Table::Cards)
            .await
            .into_iter()
            .map(CardId)
            .collect()
    }

    async fn last_modified_attribute(&self, id: AttributeId) -> Duration {
        self.last_modified(Table::Attributes, id.into_inner())
            .await
            .unwrap_or_default()
    }
    async fn last_modified_card(&self, id: CardId) -> Duration {
        self.last_modified(Table::Cards, id.into_inner())
            .await
            .unwrap_or_default()
    }

    async fn last_modified_reviews(&self, id: CardId) -> Option<Duration> {
        self.last_modified(Table::Reviews, id.into_inner()).await
    }

    async fn load_all_cards(&self) -> Vec<RawCard> {
        let cards = self.load_all_content(Table::Cards).await;
        tracing::info!("loaded cards!");
        cards
    }

    async fn save_card(&self, card: RawCard) {
        info!("lets save a card!");
        self.save_content(Table::Cards, card.id, card);
        info!("nice saved it i guess ? ");
    }

    async fn load_card(&self, id: CardId) -> Option<RawCard> {
        self.load_content(Table::Cards, id.into_inner()).await
    }

    async fn load_all_attributes(&self) -> Vec<AttributeDTO> {
        self.load_all_content(Table::Attributes).await
    }

    async fn save_attribute(&self, attribute: AttributeDTO) {
        self.save_content(Table::Attributes, attribute.id.into_inner(), attribute);
    }

    async fn load_attribute(&self, id: AttributeId) -> Option<AttributeDTO> {
        self.load_content(Table::Attributes, id.into_inner()).await
    }

    async fn delete_card(&self, id: CardId) {
        self.delete_content(Table::Cards, id.into_inner());
    }

    async fn delete_attribute(&self, id: AttributeId) {
        self.delete_content(Table::Attributes, id.into_inner());
    }

    async fn load_reviews(&self, id: CardId) -> Vec<Review> {
        let mut reviews = vec![];

        let Some(s) = self.raw_load_content(Table::Reviews, id.into_inner()).await else {
            return vec![];
        };

        for line in s.lines() {
            let (timestamp, grade) = line.split_once(' ').unwrap();
            let timestamp = Duration::from_secs(timestamp.parse().unwrap());
            let grade = Recall::from_str(grade).unwrap();
            let review = Review {
                timestamp,
                grade,
                time_spent: Duration::default(),
            };
            reviews.push(review);
        }

        reviews.sort_by_key(|r| r.timestamp);
        reviews
    }

    async fn save_reviews(&self, id: CardId, reviews: Vec<Review>) {
        let mut s = String::new();
        for r in reviews {
            let stamp = r.timestamp.as_secs().to_string();
            let grade = match r.grade {
                Recall::None => "1",
                Recall::Late => "2",
                Recall::Some => "3",
                Recall::Perfect => "4",
            };
            s.push_str(&format!("{} {}\n", stamp, grade));
        }

        info!("lets save reviews!!");
        self.raw_save_content(Table::Reviews, id.into_inner(), s);
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
