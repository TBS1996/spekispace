use speki_dto::Review;
use speki_dto::{CardId, Recall, SpekiProvider};
use speki_fs::FileProvider;
use std::time::Duration;

#[derive(Ord, PartialOrd, Eq, Hash, PartialEq, Debug, Default, Clone)]
pub struct Reviews(pub Vec<Review>);

impl Reviews {
    pub async fn load(id: CardId) -> Self {
        Self(FileProvider.load_reviews(id).await)
    }

    pub async fn save(&self, id: CardId) {
        FileProvider
            .save_reviews(id, self.clone().into_inner())
            .await;
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn into_inner(self) -> Vec<Review> {
        self.0
    }

    pub fn from_raw(reviews: Vec<Review>) -> Self {
        Self(reviews)
    }

    pub async fn add_review(&mut self, id: CardId, recall: Recall, now: Duration) {
        let review = Review {
            timestamp: now,
            grade: recall,
            time_spent: Default::default(),
        };
        FileProvider.add_review(id, review).await;
    }

    pub fn lapses_since(&self, dur: Duration, current_time: Duration) -> u32 {
        let since = current_time - dur;
        self.0.iter().fold(0, |lapses, review| match review.grade {
            Recall::None | Recall::Late => {
                if review.timestamp < since {
                    0
                } else {
                    lapses + 1
                }
            }
            Recall::Some | Recall::Perfect => 0,
        })
    }

    pub fn lapses(&self) -> u32 {
        self.0.iter().fold(0, |lapses, review| match review.grade {
            Recall::None | Recall::Late => lapses + 1,
            Recall::Some | Recall::Perfect => 0,
        })
    }

    pub fn time_since_last_review(&self, current_unix: Duration) -> Option<Duration> {
        let last = self.0.last()?;
        Some(current_unix - last.timestamp)
    }
}
