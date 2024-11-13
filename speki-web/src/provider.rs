use std::{path::PathBuf, str::FromStr, time::Duration};

use speki_dto::{
    AttributeDTO, AttributeId, CardId, Config, RawCard, Recall, Review, SpekiProvider,
};
pub struct IndexBaseProvider;
use crate::js::{self, save_file};

fn share_path() -> PathBuf {
    PathBuf::from("/foobar")
}

fn review_path() -> PathBuf {
    share_path().join("reviews")
}

fn attrs_path() -> PathBuf {
    share_path().join("attributes")
}

fn cards_path() -> PathBuf {
    share_path().join("cards")
}

use async_trait::async_trait;

#[async_trait(?Send)]
impl SpekiProvider for IndexBaseProvider {
    async fn load_all_cards(&self) -> Vec<RawCard> {
        js::load_all_files(cards_path().to_str().unwrap())
            .await
            .into_iter()
            .map(|s| toml::from_str(&s).unwrap())
            .collect()
    }

    async fn save_card(&self, card: RawCard) {
        let s: String = toml::to_string(&card).unwrap();
        let path = cards_path().join(card.id.to_string());
        js::save_file(path.to_str().unwrap(), &s);
    }

    async fn load_card(&self, id: CardId) -> Option<RawCard> {
        let path = cards_path().join(id.to_string());
        let s = js::load_file(path.to_str().unwrap()).await?;
        toml::from_str(&s).unwrap()
    }

    async fn load_all_attributes(&self) -> Vec<AttributeDTO> {
        js::load_all_files(attrs_path().to_str().unwrap())
            .await
            .into_iter()
            .map(|s| toml::from_str(&s).unwrap())
            .collect()
    }

    async fn save_attribute(&self, attribute: AttributeDTO) {
        let s: String = toml::to_string(&attribute).unwrap();
        let path = cards_path().join(attribute.id.0.to_string());
        js::save_file(path.to_str().unwrap(), &s);
    }

    async fn load_attribute(&self, id: AttributeId) -> Option<AttributeDTO> {
        let path = attrs_path().join(id.into_inner().to_string());
        let s = js::load_file(path.to_str().unwrap()).await?;
        toml::from_str(&s).unwrap()
    }

    async fn delete_card(&self, id: CardId) {
        let path = cards_path().join(id.to_string());
        js::delete_file(path.to_str().unwrap());
    }

    async fn delete_attribute(&self, id: AttributeId) {
        let path = attrs_path().join(id.into_inner().to_string());
        js::delete_file(path.to_str().unwrap());
    }

    async fn load_reviews(&self, id: CardId) -> Vec<Review> {
        let mut reviews = vec![];
        let path = review_path().join(id.to_string());

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

        let path = review_path().join(id.to_string());
        save_file(path.to_str().unwrap(), &s);
    }

    async fn load_config(&self) -> Config {
        Config
    }

    async fn save_config(&self, _config: Config) {
        todo!()
    }
}
