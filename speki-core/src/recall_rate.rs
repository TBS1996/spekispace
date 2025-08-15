use std::time::Duration;

use ledgerstore::{LedgerEvent, LedgerItem};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::card::{CardId, RecallRate};

#[derive(Clone, Copy)]
pub struct SimpleRecall;

impl SimpleRecall {
    pub fn recall_rate(&self, reviews: &History, current_unix: Duration) -> Option<RecallRate> {
        simple_recall_rate(reviews, current_unix)
    }
}

pub fn simple_recall_rate(reviews: &History, current_unix: Duration) -> Option<RecallRate> {
    let days_passed = reviews.time_since_last_review(current_unix)?;
    let stability = stability(reviews)?;
    let randomized_stability =
        randomize_factor(stability.as_secs_f32(), reviews.last().unwrap().timestamp);
    let stability = Duration::from_secs_f32(randomized_stability);
    Some(calculate_recall_rate(&days_passed, &stability))
}

/// Randomizes the flashcard factor with a factor of 0.5 to 1.4 to avoid clustering of reviews
fn randomize_factor(factor: f32, prev_review_timestamp: Duration) -> f32 {
    let rand = prev_review_timestamp.as_secs();
    let rand = rand % 10; // random number from 0 to 9
    let rand = rand as f32 / 10.; // random number from 0.0 to 0.9
    let rand = rand + 0.5; // random number from 0.5 to 1.4
    factor * rand
}

fn new_stability(
    grade: &Recall,
    time_passed: Option<Duration>,
    current_stability: Duration,
) -> Duration {
    let grade_factor = grade.get_factor();
    let time_passed = time_passed.unwrap_or(Duration::from_secs(86400));

    if grade_factor < 1.0 {
        // the grade is wrong
        time_passed.min(current_stability).mul_f32(grade_factor)
    } else {
        // the grade is correct
        let alternative_stability = time_passed.mul_f32(grade_factor);
        if alternative_stability > current_stability {
            alternative_stability
        } else {
            let interpolation_ratio =
                time_passed.as_secs_f32() / current_stability.as_secs_f32() * grade_factor;
            current_stability
                + Duration::from_secs_f32(current_stability.as_secs_f32() * interpolation_ratio)
        }
    }
}

pub fn stability(reviews: &History) -> Option<Duration> {
    let reviews = reviews.inner();
    if reviews.is_empty() {
        return None;
    }

    let mut stability = new_stability(&reviews[0].grade, None, Duration::from_secs(86400));
    let mut prev_timestamp = reviews[0].timestamp;

    for review in &reviews[1..] {
        if prev_timestamp > review.timestamp {
            return None;
        }
        let time_passed = review.timestamp - prev_timestamp; // Calculate the time passed since the previous review
        stability = new_stability(&review.grade, Some(time_passed), stability);
        prev_timestamp = review.timestamp; // Update the timestamp for the next iteration
    }

    Some(stability)
}

fn calculate_recall_rate(days_passed: &Duration, stability: &Duration) -> RecallRate {
    let base: f32 = 0.9;
    let ratio = days_passed.as_secs_f32() / stability.as_secs_f32();
    (base.ln() * ratio).exp()
}

#[derive(Ord, PartialOrd, Eq, PartialEq, Hash, Clone, Debug, Deserialize, Serialize)]
pub struct History {
    pub id: Uuid,
    pub reviews: Vec<Review>,
}

impl History {
    pub fn inner(&self) -> &Vec<Review> {
        &self.reviews
    }

    pub fn maturity_days(&self, time: Duration) -> Option<f32> {
        self.maturity(time).map(|d| d.as_secs_f32() / 86400.)
    }

    pub fn maturity(&self, time: Duration) -> Option<Duration> {
        use gkquad::single::integral;

        if self.recall_rate(time).is_none() {
            return None;
        }

        let result = integral(
            |x: f64| {
                self.recall_rate(time + Duration::from_secs_f64(x * 86400.))
                    .unwrap_or_default() as f64
            },
            0.0..1000.,
        )
        .estimate()
        .ok()?;

        let dur = Duration::from_secs_f64(result * 86400.);

        Some(dur)
    }

    pub fn last(&self) -> Option<Review> {
        self.reviews.last().cloned()
    }

    pub fn recall_rate(&self, time: Duration) -> Option<f32> {
        simple_recall_rate(self, time)
    }

    pub fn lapses_since(&self, dur: Duration, current_time: Duration) -> u32 {
        let since = current_time - dur;
        self.reviews
            .iter()
            .fold(0, |lapses, review| match review.grade {
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
        self.reviews
            .iter()
            .fold(0, |lapses, review| match review.grade {
                Recall::None | Recall::Late => lapses + 1,
                Recall::Some | Recall::Perfect => 0,
            })
    }

    pub fn time_since_last_review(&self, current_unix: Duration) -> Option<Duration> {
        let last = self.reviews.last()?;
        Some(current_unix - last.timestamp)
    }

    pub fn len(&self) -> usize {
        self.reviews.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn new(id: CardId) -> Self {
        Self {
            id,
            reviews: Default::default(),
        }
    }

    pub fn push(&mut self, review: Review) {
        self.reviews.push(review);
        self.reviews.sort_by_key(|r| r.timestamp);
    }

    pub fn insert_many(&mut self, reviews: impl IntoIterator<Item = Review>) {
        self.reviews.extend(reviews);
        self.reviews.sort_by_key(|r| r.timestamp);
        self.reviews.dedup();
    }

    pub fn merge_into(&mut self, other: Self) {
        self.insert_many(other.reviews);
    }
}

#[derive(Ord, PartialOrd, Eq, PartialEq, Hash, Clone, Debug, Default, Deserialize, Serialize)]
pub struct Review {
    // When (unix time) did the review take place?
    pub timestamp: Duration,
    // Recall grade.
    pub grade: Recall,
}

impl Review {
    pub fn is_success(&self) -> bool {
        match self.grade {
            Recall::None => false,
            Recall::Late => false,
            Recall::Some => true,
            Recall::Perfect => true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash)]
pub enum ReviewAction {
    Insert(Review),
}

pub type ReviewEvent = LedgerEvent<History>;

impl LedgerItem for History {
    type Error = ();
    type Key = CardId;
    type PropertyType = String;
    type RefType = String;
    type Modifier = ReviewAction;

    fn inner_run_event(mut self, event: ReviewAction) -> Result<Self, ()> {
        let review = match event {
            ReviewAction::Insert(review) => Review {
                timestamp: review.timestamp,
                grade: review.grade,
            },
        };

        self.push(review);

        Ok(self)
    }

    fn new_default(id: CardId) -> Self {
        Self::new(id)
    }

    fn item_id(&self) -> CardId {
        self.id
    }
}

#[derive(
    Ord, PartialOrd, Eq, PartialEq, Hash, Deserialize, Serialize, Debug, Default, Clone, Copy,
)]
#[serde(rename_all = "lowercase")]
pub enum Recall {
    #[default]
    None,
    Late,
    Some,
    Perfect,
}

impl Recall {
    pub fn get_factor(&self) -> f32 {
        match self {
            Recall::None => 0.1,
            Recall::Late => 0.25,
            Recall::Some => 2.,
            Recall::Perfect => 3.,
        }
    }

    pub fn desc(&self) -> &'static str {
        match self {
            Recall::None => "No recall even after seeing the answer",
            Recall::Late => "No/failed recall but recognized the answer when it was revelead",
            Recall::Some => "Correcct recall but it took some effort",
            Recall::Perfect => "No hesitation, perfect recall",
        }
    }
}

impl std::str::FromStr for Recall {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "1" => Ok(Self::None),
            "2" => Ok(Self::Late),
            "3" => Ok(Self::Some),
            "4" => Ok(Self::Perfect),
            _ => Err(()),
        }
    }
}
