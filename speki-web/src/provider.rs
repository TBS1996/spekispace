use std::{path::PathBuf, str::FromStr, time::Duration};

use speki_dto::{
    AttributeDTO, AttributeId, CardId, Config, RawCard, Recall, Review, SpekiProvider,
};
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

use crate::{
    js::{self, save_file},
    log_to_console,
};

use async_trait::async_trait;

#[async_trait(?Send)]
impl SpekiProvider for IndexBaseProvider {
    async fn load_all_cards(&self) -> Vec<RawCard> {
        let cards = js::load_all_files(self.cards_path().to_str().unwrap())
            .await
            .into_iter()
            .map(|s| toml::from_str(&s).unwrap())
            .collect();
        log_to_console("loaded cards!");
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
        save_file(path.to_str().unwrap(), &s);
    }

    async fn load_config(&self) -> Config {
        Config
    }

    async fn save_config(&self, _config: Config) {
        todo!()
    }
}
