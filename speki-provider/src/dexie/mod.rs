use std::str::FromStr;
use std::time::Duration;

use speki_dto::Config;
use speki_dto::{AttributeDTO, AttributeId, CardId, RawCard, Recall, Review, SpekiProvider};

mod js;

pub struct DexieProvider;

use async_trait::async_trait;
use tracing::info;

#[async_trait(?Send)]
impl SpekiProvider for DexieProvider {
    async fn load_card_ids(&self) -> Vec<CardId> {
        js::load_ids()
            .await
            .into_iter()
            .map(|id| CardId(id.parse().unwrap()))
            .collect()
    }

    async fn last_modified_card(&self, id: CardId) -> Duration {
        js::last_modified(&id.into_inner().to_string())
            .await
            .unwrap()
    }

    async fn last_modified_reviews(&self, id: CardId) -> Option<Duration> {
        js::last_modified(&id.to_string()).await
    }

    async fn load_all_cards(&self) -> Vec<RawCard> {
        let cards = js::load_all_files()
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
        js::save_file(&id.to_string(), &s);
    }

    async fn load_card(&self, id: CardId) -> Option<RawCard> {
        let s = js::load_file(&id.to_string()).await?;
        toml::from_str(&s).unwrap()
    }

    async fn load_all_attributes(&self) -> Vec<AttributeDTO> {
        js::load_all_files()
            .await
            .into_iter()
            .map(|s| toml::from_str(&s).unwrap())
            .collect()
    }

    async fn save_attribute(&self, attribute: AttributeDTO) {
        let id = attribute.id;
        let s: String = toml::to_string(&attribute).unwrap();
        js::save_file(&id.to_string(), &s);
    }

    async fn load_attribute(&self, id: AttributeId) -> Option<AttributeDTO> {
        let s = js::load_file(&id.to_string()).await?;
        toml::from_str(&s).unwrap()
    }

    async fn delete_card(&self, id: CardId) {
        js::delete_file(&id.to_string());
    }

    async fn delete_attribute(&self, id: AttributeId) {
        js::delete_file(&id.to_string());
    }

    async fn load_reviews(&self, id: CardId) -> Vec<Review> {
        let mut reviews = vec![];

        let Some(s) = js::load_reviews(&id.to_string()).await else {
            return vec![];
        };

        info!("string: {s}");

        for line in s.lines() {
            let (timestamp, grade) = line.split_once(' ').unwrap();
            info!("{timestamp}, {grade}");
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

        js::save_reviews(&id.to_string(), &s);
    }

    async fn load_config(&self) -> Config {
        Config
    }

    async fn save_config(&self, _config: Config) {
        todo!()
    }
}
