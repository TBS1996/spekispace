use async_trait::async_trait;
use speki_dto::{
    AttributeDTO, AttributeId, CardId, Config, RawCard, Recall, Review, SpekiProvider,
};
use std::{path::PathBuf, str::FromStr, time::Duration};

pub struct IndexBaseProvider {
    repo: PathBuf,
}

impl IndexBaseProvider {
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
}

mod js {
    use gloo_utils::format::JsValueSerdeExt;
    use js_sys::Promise;
    use serde_json::Value;
    use std::time::Duration;
    use tracing::info;
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(module = "/assets/utils.js")]
    extern "C" {
        fn loadFile(path: &JsValue) -> Promise;
    }

    /*
        fn lastModified(path: &JsValue) -> Promise;
        fn loadAllFiles(path: &JsValue) -> Promise;
        fn saveFile(path: &JsValue, content: &JsValue);
        fn deleteFile(path: &JsValue);
        fn loadFilenames(path: &JsValue) -> Promise;
    async fn promise_to_val(promise: Promise) -> Value {
        let future = wasm_bindgen_futures::JsFuture::from(promise);
        let jsvalue = future.await.unwrap();
        jsvalue.into_serde().unwrap()
    }

    pub async fn load_filenames(path: &str) -> Vec<String> {
        let path = JsValue::from_str(path);
        let val = promise_to_val(loadFilenames(&path)).await;
        info!("{}", &val);

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

    pub async fn last_modified(path: &str) -> Option<Duration> {
        let path = JsValue::from_str(path);
        let val = promise_to_val(lastModified(&path)).await;
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

    pub async fn load_all_files(path: &str) -> Vec<String> {
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

    pub fn delete_file(path: &str) {
        let path = JsValue::from_str(path);
        deleteFile(&path);
    }

    pub async fn load_file(path: &str) -> Option<String> {
        let path = JsValue::from_str(path);
        let val = promise_to_val(loadFile(&path)).await;

        match val {
            Value::Null => None,
            Value::String(s) => Some(s.clone()),
            other => panic!("invalid type: {}", other),
        }
    }

    pub fn save_file(path: &str, content: &str) {
        let path = JsValue::from_str(path);
        let content = JsValue::from_str(content);
        saveFile(&path, &content);
    }
    */
}

/*
#[async_trait(?Send)]
impl SpekiProvider for IndexBaseProvider {
    async fn load_card_ids(&self) -> Vec<CardId> {
        js::load_filenames(self.cards_path().to_str().unwrap())
            .await
            .into_iter()
            .map(|id| CardId(id.parse().unwrap()))
            .collect()
    }

    async fn last_modified_card(&self, id: CardId) -> Duration {
        let path = self.cards_path().join(id.0.to_string());
        js::last_modified(path.to_str().unwrap()).await.unwrap()
    }

    async fn last_modified_reviews(&self, id: CardId) -> Option<Duration> {
        let path = self.review_path().join(id.0.to_string());
        js::last_modified(path.to_str().unwrap()).await
    }

    async fn load_all_cards(&self) -> Vec<RawCard> {
        let cards = js::load_all_files(self.cards_path().to_str().unwrap())
            .await
            .into_iter()
            .map(|s| toml::from_str(&s).unwrap())
            .collect();
        tracing::info!("loaded cards!");
        cards
    }

    async fn save_card(&self, card: RawCard) {
        let s: String = toml::to_string(&card).unwrap();
        let path = self.cards_path().join(card.id.to_string());
        js::save_file(path.to_str().unwrap(), &s);
    }

    async fn load_card(&self, id: CardId) -> Option<RawCard> {
        let path = self.cards_path().join(id.to_string());
        let s = js::load_file(path.to_str().unwrap()).await?;
        toml::from_str(&s).unwrap()
    }

    async fn load_all_attributes(&self) -> Vec<AttributeDTO> {
        js::load_all_files(self.attrs_path().to_str().unwrap())
            .await
            .into_iter()
            .map(|s| toml::from_str(&s).unwrap())
            .collect()
    }

    async fn save_attribute(&self, attribute: AttributeDTO) {
        let s: String = toml::to_string(&attribute).unwrap();
        let path = self.cards_path().join(attribute.id.0.to_string());
        js::save_file(path.to_str().unwrap(), &s);
    }

    async fn load_attribute(&self, id: AttributeId) -> Option<AttributeDTO> {
        let path = self.attrs_path().join(id.into_inner().to_string());
        let s = js::load_file(path.to_str().unwrap()).await?;
        toml::from_str(&s).unwrap()
    }

    async fn delete_card(&self, id: CardId) {
        let path = self.cards_path().join(id.to_string());
        js::delete_file(path.to_str().unwrap());
    }

    async fn delete_attribute(&self, id: AttributeId) {
        let path = self.attrs_path().join(id.into_inner().to_string());
        js::delete_file(path.to_str().unwrap());
    }

    async fn load_reviews(&self, id: CardId) -> Vec<Review> {
        let mut reviews = vec![];
        let path = self.review_path().join(id.to_string());

        let Some(s) = js::load_file(path.to_str().unwrap()).await else {
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

        let path = self.review_path().join(id.to_string());
        js::save_file(path.to_str().unwrap(), &s);
    }

    async fn load_config(&self) -> Config {
        Config
    }

    async fn save_config(&self, _config: Config) {
        todo!()
    }
}


*/
