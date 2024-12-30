use std::time::Duration;

use speki_dto::{History, Recall};

use crate::{card::RecallRate, RecallCalc};

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
