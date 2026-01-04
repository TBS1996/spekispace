pub mod ml;

use std::time::Duration;

use ledgerstore::{LedgerEvent, LedgerItem};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    card::{CardId, RecallRate},
    recall_rate::ml::classic::Trained,
};

pub trait Recaller {
    fn eval(&self, id: CardId, reviews: &[Review], time: Duration) -> Option<f32>;
}

#[derive(Clone)]
pub struct AvgRecall {
    pub trained: Trained,
    pub simple: FSRS,
    /// Weight on the ML model. 1.0 = only ML, 0.0 = only base
    pub alpha: f32,
}

impl Default for AvgRecall {
    fn default() -> Self {
        Self {
            trained: Trained::from_static(),
            simple: FSRS,
            alpha: 0.75,
        }
    }
}

impl Recaller for AvgRecall {
    fn eval(&self, id: CardId, reviews: &[Review], time: Duration) -> Option<f32> {
        let mut the_reviews: Vec<Review> = vec![];
        for review in reviews {
            if review.timestamp < time {
                the_reviews.push(review.clone());
            }
        }

        let p_ml = self.trained.eval(id, &the_reviews, time)?;
        let p_man = self.simple.eval(id, &the_reviews, time)?;
        Some(self.alpha * p_ml + (1.0 - self.alpha) * p_man)
    }
}

impl Recaller for SimpleRecall {
    fn eval(&self, _id: CardId, reviews: &[Review], time: Duration) -> Option<f32> {
        let mut the_reviews: Vec<Review> = vec![];
        for review in reviews {
            if review.timestamp < time {
                the_reviews.push(review.clone());
            }
        }

        self.recall_rate(&the_reviews, time)
    }
}

impl Recaller for Trained {
    fn eval(&self, _id: CardId, reviews: &[Review], time: Duration) -> Option<f32> {
        self.recall_rate(reviews, time).map(|x| x as f32)
    }
}

/// Stolen from: https://borretti.me/article/implementing-fsrs-in-100-lines
#[derive(Clone, Copy)]
pub struct FSRS;

impl FSRS {
    const C: f32 = -0.5;
    const F: f32 = 19.0 / 81.0;
    const FORGOT: f32 = 0.40255;
    const _HARD: f32 = 1.18385;
    const GOOD: f32 = 3.173;
    const EASY: f32 = 15.69105;

    pub const W: [f32; 19] = [
        0.40255, 1.18385, 3.173, 15.69105, 7.1949, 0.5345, 1.4604, 0.0046, 1.54575, 0.1192,
        1.01925, 1.9395, 0.11, 0.29605, 2.2698, 0.2315, 2.9898, 0.51655, 0.6621,
    ];

    fn recall_rate(time: Duration, stability: Duration) -> f32 {
        (1.0 + Self::F * (time.as_secs_f32() / stability.as_secs_f32())).powf(Self::C)
    }

    fn s_success(d: f32, stability: Duration, r: f32, g: Recall) -> Duration {
        let stability = stability.as_secs_f32() / 86400.;
        let t_d = 11.0 - d;
        let t_s = stability.powf(-Self::W[9]);
        let t_r = f32::exp(Self::W[10] * (1.0 - r)) - 1.0;
        let h = if g == Recall::Some { Self::W[15] } else { 1.0 };
        let b = if g == Recall::Perfect {
            Self::W[16]
        } else {
            1.0
        };
        let c = f32::exp(Self::W[8]);
        let alpha = 1.0 + t_d * t_s * t_r * h * b * c;
        Duration::from_secs_f32(stability * alpha * 86400.)
    }

    fn recall_factor(recall: Recall) -> f32 {
        match recall {
            Recall::None => 1.0,
            Recall::Late => 1.5,
            Recall::Some => 3.0,
            Recall::Perfect => 4.0,
        }
    }

    fn d_0(g: Recall) -> f32 {
        let g: f32 = Self::recall_factor(g);
        Self::clamp_d(Self::W[4] - f32::exp(Self::W[5] * (g - 1.0)) + 1.0)
    }

    fn clamp_d(d: f32) -> f32 {
        d.clamp(1.0, 10.0)
    }

    fn stability(d: f32, stability: Duration, r: f32, g: Recall) -> Duration {
        if !g.is_success() {
            Self::s_fail(d, stability, r)
        } else {
            Self::s_success(d, stability, r, g)
        }
    }

    fn s_fail(d: f32, stability: Duration, r: f32) -> Duration {
        let stability = stability.as_secs_f32() / 86400.;
        let d_f = d.powf(-Self::W[12]);
        let s_f = (stability + 1.0).powf(Self::W[13]) - 1.0;
        let r_f = f32::exp(Self::W[14] * (1.0 - r));
        let c_f = Self::W[11];
        let s_f = d_f * s_f * r_f * c_f;
        Duration::from_secs_f32(f32::min(s_f, stability) * 86400.)
    }

    fn s_0(g: Recall) -> Duration {
        let days = match g {
            Recall::None => Self::FORGOT,
            Recall::Late => Self::FORGOT,
            Recall::Some => Self::GOOD,
            Recall::Perfect => Self::EASY,
        };

        Duration::from_secs_f32(days * 86400.)
    }

    fn difficulty(d: f32, g: Recall) -> f32 {
        Self::clamp_d(Self::W[7] * Self::d_0(Recall::Perfect) + (1.0 - Self::W[7]) * Self::dp(d, g))
    }

    fn dp(d: f32, g: Recall) -> f32 {
        d + Self::delta_d(g) * ((10.0 - d) / 9.0)
    }

    fn delta_d(g: Recall) -> f32 {
        let g: f32 = Self::recall_factor(g);
        -Self::W[6] * (g - 3.0)
    }
}

impl Recaller for FSRS {
    fn eval(&self, _id: CardId, reviews: &[Review], time: Duration) -> Option<f32> {
        let mut the_reviews: Vec<Review> = vec![];
        for review in reviews {
            if review.timestamp < time {
                the_reviews.push(review.clone());
            }
        }

        let reviews = the_reviews;

        let mut iter = reviews.iter();

        let first_review = iter.next()?;

        let mut stability = Self::s_0(first_review.grade);
        let mut difficulty = Self::d_0(first_review.grade);
        let mut prev_review = first_review.timestamp;

        for review in iter {
            let time_passed = review.timestamp - prev_review;
            let recall = Self::recall_rate(time_passed, stability);
            stability = Self::stability(difficulty, stability, recall, review.grade);
            difficulty = Self::difficulty(difficulty, review.grade);
            prev_review = review.timestamp;
        }

        Some(Self::recall_rate(time - prev_review, stability))
    }
}

#[derive(Clone, Copy)]
pub struct SimpleRecall;

impl SimpleRecall {
    pub fn recall_rate(&self, reviews: &[Review], current_unix: Duration) -> Option<RecallRate> {
        simple_recall_rate(&reviews, current_unix)
    }
}

pub fn simple_recall_rate(reviews: &[Review], current_unix: Duration) -> Option<RecallRate> {
    let days_passed = current_unix - reviews.last()?.timestamp;
    let stability = stability(reviews)?;
    Some(calculate_recall_rate(&days_passed, &stability))
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

pub fn stability(reviews: &[Review]) -> Option<Duration> {
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
    if stability.is_zero() {
        return 0.0;
    };

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

    pub fn rate_vs_result(&self, algo: impl Recaller) -> Vec<(f32, bool)> {
        let mut out = vec![];

        let mut reviews: Vec<Review> = vec![];

        for review in &self.reviews {
            if !reviews.is_empty() {
                let rate = algo.eval(self.id, &reviews, review.timestamp).unwrap();
                let recalled = review.is_success();
                out.push((rate, recalled));
            }

            reviews.push(review.clone());
        }

        out
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

    /// Recall rate at a given time.
    /// includes randomness so reviews are spread out a bit
    /// otherwise if you do a large number of reviews at same time
    /// all the reviews will come again at same time.
    pub fn recall_rate(&self, time: Duration) -> Option<f32> {
        let factor = self.id.as_u128() % 100; // 0 -> 100
        let factor = factor as f32 / 100.; // 0. -> 1.0
        let _factor = 1.8 - factor; // 0.8 -> 1.8
        let factor = 1.0;

        simple_recall_rate(&self.reviews, time).map(|recall| recall * factor)
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
        self.grade.is_success()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash)]
pub enum ReviewAction {
    Insert(Review),
    Remove(Duration),
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
            ReviewAction::Remove(timestamp) => {
                self.reviews.retain(|r| r.timestamp != timestamp);
                return Ok(self);
            }
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

    pub fn is_success(&self) -> bool {
        match self {
            Recall::None => false,
            Recall::Late => false,
            Recall::Some => true,
            Recall::Perfect => true,
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
