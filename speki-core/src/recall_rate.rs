use std::time::Duration;

use serde::{Deserialize, Serialize};
use speki_dto::{LedgerEvent, RunLedger};
use uuid::Uuid;

use crate::{
    card::{CardId, RecallRate},
    RecallCalc,
};

pub struct SimpleRecall;

impl RecallCalc for SimpleRecall {
    fn recall_rate(&self, reviews: &History, current_unix: Duration) -> Option<RecallRate> {
        simple_recall_rate(reviews, current_unix)
    }
}

fn simple_recall_rate(reviews: &History, current_unix: Duration) -> Option<RecallRate> {
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

fn stability(reviews: &History) -> Option<Duration> {
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

    pub fn last(&self) -> Option<Review> {
        self.reviews.last().cloned()
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
        self.reviews.sort_by_key(|r|r.timestamp);
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
    // How long you spent before attempting recall.
    pub time_spent: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash)]
pub struct ReviewEvent {
pub    id: CardId,
 pub   grade: Recall,
  pub  timestamp: Duration,
}


impl LedgerEvent for ReviewEvent {
    fn id(&self) -> String {
        self.id.to_string()
    }
}


impl RunLedger<ReviewEvent> for History {
    type Error = ();

    fn run_event(mut self, event: ReviewEvent) -> Result<Self, ()> {
        let review = Review {
            timestamp: event.timestamp,
            grade: event.grade,
            time_spent: Default::default(),
        };

        self.push(review);

        Ok(self)
    }

    fn derive_events(&self) -> Vec<ReviewEvent> {
        let mut actions: Vec<ReviewEvent> = vec![];

        for review in &self.reviews {
            let event = ReviewEvent {
                id: self.id,
                grade: review.grade,
                timestamp: review.timestamp,
            };

            actions.push(event);
        }

        actions
    }
    
    fn new_default(id: String) -> Self {
        Self::new(id.parse().unwrap())
    }
    
    fn item_id(&self) -> String {
        self.id.to_string()
    }
}


#[derive(
    Ord, PartialOrd, Eq, PartialEq, Hash, Deserialize, Serialize, Debug, Default, Clone, Copy,
)]
#[serde(rename_all = "lowercase")]
pub enum Recall {
    // No recall, not even when you saw the answer.
    #[default]
    None,
    // No recall, but you remember the answer when you read it.
    Late,
    // Struggled but you got the answer right or somewhat right.
    Some,
    // No hesitation, perfect recall.
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