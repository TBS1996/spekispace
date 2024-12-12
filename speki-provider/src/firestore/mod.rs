pub struct FirestoreProvider;

use async_trait::async_trait;

use wasm_bindgen::JsValue;

#[async_trait(?Send)]
impl SpekiProvider for FirestoreProvider {
    async fn load_card_ids(&self) -> Vec<CardId> {
        js::load_ids(Table::Cards)
            .await
            .into_iter()
            .map(|id| CardId(id))
            .collect()
    }

    async fn last_modified_card(&self, id: CardId) -> Duration {
        js::last_modified(Table::Cards, &id.to_string())
            .await
            .unwrap()
    }

    async fn last_modified_attribute(&self, id: AttributeId) -> Duration {
        panic!()
    }

    async fn last_modified_reviews(&self, id: CardId) -> Option<Duration> {
        js::last_modified(Table::Reviews, &id.to_string()).await
    }

    async fn load_all_cards(&self) -> Vec<RawCard> {
        let cards = js::load_all_files(Table::Cards)
            .await
            .into_iter()
            .map(|s| toml::from_str(&s).unwrap())
            .collect();
        tracing::info!("loaded cards!");
        cards
    }

    async fn save_card(&self, card: RawCard) {
        let id = card.id;
        let s: String = toml::to_string(&card).unwrap();
        js::save_content(Table::Cards, &id.to_string(), &s);
    }

    async fn load_card(&self, id: CardId) -> Option<RawCard> {
        let s = js::load_content(Table::Cards, &id.to_string()).await?;
        toml::from_str(&s).unwrap()
    }

    async fn load_all_attributes(&self) -> Vec<AttributeDTO> {
        js::load_all_files(Table::Attributes)
            .await
            .into_iter()
            .map(|s| toml::from_str(&s).unwrap())
            .collect()
    }

    async fn save_attribute(&self, attribute: AttributeDTO) {
        let id = attribute.id;
        let s: String = toml::to_string(&attribute).unwrap();
        js::save_content(Table::Attributes, &id.to_string(), &s);
    }

    async fn load_attribute(&self, id: AttributeId) -> Option<AttributeDTO> {
        let s = js::load_content(Table::Attributes, &id.to_string()).await?;
        toml::from_str(&s).unwrap()
    }

    async fn delete_card(&self, id: CardId) {
        js::delete_file(Table::Cards, &id.to_string());
    }

    async fn delete_attribute(&self, id: AttributeId) {
        js::delete_file(Table::Attributes, &id.to_string());
    }

    async fn load_reviews(&self, id: CardId) -> Vec<Review> {
        let mut reviews = vec![];

        let Some(s) = js::load_content(Table::Reviews, &id.to_string()).await else {
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

        js::save_content(Table::Reviews, &id.to_string(), &s);
    }

    async fn load_config(&self) -> Config {
        Config
    }

    async fn save_config(&self, _config: Config) {
        todo!()
    }
}
