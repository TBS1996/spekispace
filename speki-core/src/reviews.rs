use crate::common::current_time;
use crate::paths::{self, get_review_path};
use serde::{Deserialize, Serialize};
use speki_dto::CardId;
use std::fs::{self, File};
use std::io::Write;
use std::str::FromStr;
use std::time::Duration;

#[derive(Ord, PartialOrd, Eq, Hash, PartialEq, Debug, Default, Clone)]
pub struct Reviews(pub Vec<Review>);

impl Reviews {
    pub fn load(id: CardId) -> Option<Self> {
        let path = paths::get_review_path().join(id.to_string());
        if path.exists() {
            let s = fs::read_to_string(path).unwrap();
            Some(Self::from_str(&s))
        } else {
            None
        }
    }

    pub fn save(&self, id: CardId) {
        let path = get_review_path();
        fs::create_dir_all(&path).unwrap();
        let path = path.join(id.to_string());
        let mut s = String::new();
        for r in &self.0 {
            let stamp = r.timestamp.as_secs().to_string();
            let grade = match r.grade {
                Recall::None => "1",
                Recall::Late => "2",
                Recall::Some => "3",
                Recall::Perfect => "4",
            };
            s.push_str(&format!("{} {}\n", stamp, grade));
        }

        let mut f = File::create(&path).unwrap();
        f.write_all(&mut s.as_bytes()).unwrap();
    }

    pub fn from_str(s: &str) -> Self {
        let mut reviews = vec![];
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

        Self(reviews)
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

    pub fn add_review(&mut self, review: Review) {
        self.0.push(review);
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
        self.0.last().map(|review| review.time_passed(current_unix))
    }
}

#[derive(Ord, PartialOrd, Eq, PartialEq, Hash, Clone, Debug, Default)]
pub struct Review {
    // When (unix time) did the review take place?
    pub timestamp: Duration,
    // Recall grade.
    pub grade: Recall,
    // How long you spent before attempting recall.
    pub time_spent: Duration,
}

impl Review {
    pub fn new(grade: Recall, time_spent: Duration) -> Self {
        Self {
            timestamp: current_time(),
            grade,
            time_spent,
        }
    }

    pub fn time_passed(&self, current_unix: Duration) -> Duration {
        let unix = self.timestamp;
        current_unix.checked_sub(unix).unwrap_or_default()
    }
}

#[derive(Ord, PartialOrd, Eq, PartialEq, Hash, Deserialize, Serialize, Debug, Default, Clone)]
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
