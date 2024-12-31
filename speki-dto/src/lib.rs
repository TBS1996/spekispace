use omtrent::TimeStamp;
use serde::de::DeserializeOwned;
use serde::{ser::SerializeSeq, Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Debug;
use std::str::FromStr;
use std::time::Duration;
use tracing::{info, warn};
use uuid::Uuid;

#[derive(Clone, Copy, Debug)]
pub enum Cty {
    Attribute,
    Review,
    Card,
}

impl Item for History {
    fn last_modified(&self) -> Duration {
        self.reviews
            .iter()
            .max()
            .map(|rev| rev.timestamp)
            .unwrap_or_default()
    }

    fn id(&self) -> Uuid {
        self.id
    }

    fn serialize(&self) -> String {
        toml::to_string(self).unwrap()
    }

    fn identifier() -> Cty {
        Cty::Review
    }

    fn merge(mut self, other: Self) -> MergeRes<Self>
    where
        Self: Sized,
    {
        debug_assert!(self.id == other.id);

        let selflen = self.reviews.len();
        let otherlen = other.reviews.len();

        if selflen == otherlen {
            return MergeRes::Neither;
        }

        let merged = {
            self.merge_into(other);
            self
        };

        let mergedlen = merged.len();

        if mergedlen == selflen {
            MergeRes::Right(merged)
        } else if mergedlen == otherlen {
            MergeRes::Left(merged)
        } else {
            MergeRes::Both(merged)
        }
    }

    fn deleted(&self) -> bool {
        false
    }

    fn set_delete(&mut self) {}

    fn deserialize(id: Uuid, s: String) -> Self {
        parse_history(s, id)
    }
}

impl Item for AttributeDTO {
    fn last_modified(&self) -> Duration {
        self.last_modified
    }

    fn id(&self) -> Uuid {
        self.id
    }

    fn serialize(&self) -> String {
        toml::to_string(self).unwrap()
    }

    fn identifier() -> Cty {
        Cty::Attribute
    }

    fn deleted(&self) -> bool {
        self.deleted
    }

    fn set_delete(&mut self) {
        self.deleted = true;
    }

    fn deserialize(_id: Uuid, s: String) -> Self {
        toml::from_str(&s).unwrap()
    }
}

impl Item for RawCard {
    fn last_modified(&self) -> Duration {
        self.last_modified
    }

    fn deserialize(_id: Uuid, s: String) -> Self {
        toml::from_str(&s).unwrap()
    }

    fn id(&self) -> Uuid {
        self.id
    }

    fn serialize(&self) -> String {
        toml::to_string(self).unwrap()
    }

    fn identifier() -> Cty {
        Cty::Card
    }

    fn merge(self, other: Self) -> MergeRes<Self>
    where
        Self: Sized,
    {
        let selfmod = self.last_modified();
        let othermod = other.last_modified();

        if self.deleted && !other.deleted {
            MergeRes::Right(self)
        } else if !self.deleted && other.deleted {
            MergeRes::Left(other)
        } else if selfmod > othermod {
            MergeRes::Right(self)
        } else if selfmod < othermod {
            MergeRes::Left(other)
        } else {
            MergeRes::Neither
        }
    }

    fn deleted(&self) -> bool {
        self.deleted
    }

    fn set_delete(&mut self) {
        self.deleted = true;
    }
}

impl<T: Item> From<T> for Record {
    fn from(value: T) -> Self {
        value.into_record()
    }
}

pub trait Item: DeserializeOwned + Sized + Send + Clone + 'static {
    fn deleted(&self) -> bool;
    fn set_delete(&mut self);

    fn last_modified(&self) -> Duration;
    fn id(&self) -> Uuid;
    fn serialize(&self) -> String;
    fn deserialize(id: Uuid, s: String) -> Self;
    fn identifier() -> Cty;
    /// Returns whehter hte returned value should be saved to self, other, enither, or both.
    fn merge(self, other: Self) -> MergeRes<Self>
    where
        Self: Sized,
    {
        if self.deleted() && !other.deleted() {
            MergeRes::Both(self)
        } else if !self.deleted() && other.deleted() {
            MergeRes::Both(other)
        } else if self.last_modified() > other.last_modified() {
            MergeRes::Right(self)
        } else if self.last_modified() < other.last_modified() {
            MergeRes::Left(other)
        } else {
            MergeRes::Neither
        }
    }

    fn into_record(self) -> Record
    where
        Self: Sized,
    {
        let id = self.id().to_string();
        let last_modified = self.last_modified().as_secs();
        let content = self.serialize();

        Record {
            id,
            content,
            last_modified,
        }
    }
}

pub enum MergeRes<T> {
    Left(T),
    Right(T),
    Both(T),
    Neither,
}

#[derive(Ord, PartialOrd, Eq, PartialEq, Hash, Clone, Debug, Deserialize, Serialize)]
pub struct History {
    id: Uuid,
    reviews: Vec<Review>,
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
            reviews: vec![],
        }
    }

    pub fn push(&mut self, review: Review) {
        self.reviews.push(review);
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

#[derive(Debug, Deserialize, Clone)]
pub struct Record {
    pub id: String,
    pub content: String,
    pub last_modified: u64,
}

#[async_trait::async_trait(?Send)]
pub trait SpekiProvider<T: Item>: Sync {
    async fn load_record(&self, id: Uuid, ty: Cty) -> Option<Record>;
    async fn load_all_records(&self, ty: Cty) -> HashMap<Uuid, Record>;
    async fn save_record(&self, ty: Cty, record: Record);

    async fn load_ids(&self) -> Vec<Uuid> {
        self.load_all_records(T::identifier())
            .await
            .into_keys()
            .collect()
    }

    async fn load_item(&self, id: Uuid) -> Option<T> {
        let record = self.load_record(id, T::identifier()).await?;
        let val: T = toml::from_str(&record.content).unwrap();
        Some(val)
    }

    async fn load_all(&self) -> HashMap<Uuid, T> {
        info!("loading all for: {:?}", T::identifier());
        let map = self.load_all_records(T::identifier()).await;
        let mut outmap = HashMap::new();

        for (key, val) in map {
            match toml::from_str(&val.content) {
                Ok(val) => {
                    let _ = outmap.insert(key, val);
                }
                Err(e) => warn!("failed to deserialize: {:?}", e),
            }
        }

        outmap
    }

    async fn save_item(&self, item: T) {
        let record: Record = item.into();
        self.save_record(T::identifier(), record).await;
    }

    async fn save_records(&self, ty: Cty, records: Vec<Record>) {
        for record in records {
            self.save_record(ty, record).await;
        }
    }
}

pub async fn sync<T: Item>(left: impl SpekiProvider<T>, right: impl SpekiProvider<T>) {
    info!("starting sync of: {:?}", T::identifier());
    let mut left_update = vec![];
    let mut right_update = vec![];

    let mut map1 = left.load_all().await;
    let mut map2 = right.load_all().await;

    let mut ids: HashSet<Uuid> = map1.keys().map(|key| *key).collect();
    ids.extend(map2.keys());

    for id in &ids {
        info!("syncing card");
        let id = *id;
        match (map1.remove(&id), map2.remove(&id)) {
            (None, None) => panic!(),
            (None, Some(card)) => {
                left_update.push(card);
            }
            (Some(card), None) => {
                right_update.push(card);
            }
            (Some(card1), Some(card2)) => match card1.merge(card2) {
                MergeRes::Both(card) => {
                    left_update.push(card.clone());
                    right_update.push(card);
                }
                MergeRes::Left(card) => {
                    left_update.push(card);
                }
                MergeRes::Right(card) => {
                    right_update.push(card);
                }
                MergeRes::Neither => {}
            },
        }
    }

    left.save_records(
        Cty::Card,
        left_update
            .into_iter()
            .map(|card| card.into_record())
            .collect(),
    )
    .await;

    right
        .save_records(
            Cty::Card,
            right_update
                .into_iter()
                .map(|card| card.into_record())
                .collect(),
        )
        .await;
}

fn parse_history(s: String, id: Uuid) -> History {
    if let Ok(history) = toml::from_str(&s) {
        history
    } else {
        History {
            id,
            reviews: legacy_parse_history(s),
        }
    }
}

fn legacy_parse_history(s: String) -> Vec<Review> {
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
    reviews
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Config;

#[derive(Serialize, Deserialize, Debug, Clone, Default, Copy, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CType {
    Instance,
    #[default]
    Normal,
    Unfinished,
    Attribute,
    Class,
    Statement,
    Event,
}

pub type CardId = Uuid;

#[derive(Ord, PartialOrd, Eq, Hash, PartialEq, Debug, Clone)]
pub enum BackSide {
    Text(String),
    Card(CardId),
    List(Vec<CardId>),
    Time(TimeStamp),
    Trivial, // Answer is obvious, used when card is more of a dependency anchor
    Invalid, // A reference card was deleted
}

impl Default for BackSide {
    fn default() -> Self {
        Self::Text(Default::default())
    }
}

impl From<String> for BackSide {
    fn from(s: String) -> Self {
        if let Ok(uuid) = Uuid::parse_str(&s) {
            Self::Card(uuid)
        } else if let Some(timestamp) = TimeStamp::from_string(s.clone()) {
            Self::Time(timestamp)
        } else if s.as_str() == Self::INVALID_STR {
            Self::Invalid
        } else {
            Self::Text(s)
        }
    }
}

impl BackSide {
    pub const INVALID_STR: &'static str = "__INVALID__";

    pub fn is_empty_text(&self) -> bool {
        if let Self::Text(s) = self {
            s.is_empty()
        } else {
            false
        }
    }

    pub fn invalidate_if_has_ref(&mut self, dep: CardId) {
        let has_ref = match self {
            BackSide::Card(card_id) => card_id == &dep,
            BackSide::List(vec) => vec.contains(&dep),
            BackSide::Text(_) => false,
            BackSide::Time(_) => false,
            BackSide::Trivial => false,
            BackSide::Invalid => false,
        };

        if has_ref {
            *self = Self::Invalid;
        }
    }

    pub fn is_ref(&self) -> bool {
        matches!(self, Self::Card(_))
    }

    pub fn as_card(&self) -> Option<CardId> {
        if let Self::Card(card) = self {
            Some(*card)
        } else {
            None
        }
    }

    pub fn to_string(&self) -> String {
        let mut s = serde_json::to_string(self).unwrap();
        s.remove(0);
        s.pop();
        s
    }

    pub fn dependencies(&self) -> BTreeSet<CardId> {
        let mut set = BTreeSet::default();
        match self {
            BackSide::Text(_) => {}
            BackSide::Card(card_id) => {
                let _ = set.insert(*card_id);
            }
            BackSide::List(vec) => {
                set.extend(vec.iter());
            }
            BackSide::Time(_) => {}
            BackSide::Trivial => {}
            BackSide::Invalid => {}
        }

        set
    }
}

impl<'de> Deserialize<'de> for BackSide {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;

        match value {
            Value::Array(arr) => {
                let mut ids = Vec::new();
                for item in arr {
                    if let Value::String(ref s) = item {
                        if let Ok(uuid) = Uuid::parse_str(s) {
                            ids.push(uuid);
                        } else {
                            return Err(serde::de::Error::custom("Invalid UUID in array"));
                        }
                    } else {
                        return Err(serde::de::Error::custom("Expected string in array"));
                    }
                }
                Ok(BackSide::List(ids))
            }
            Value::Bool(_) => Ok(BackSide::Trivial),
            Value::String(s) => Ok(s.into()),
            _ => Err(serde::de::Error::custom("Expected a string or an array")),
        }
    }
}

impl Serialize for BackSide {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match *self {
            BackSide::Trivial => serializer.serialize_bool(false),
            BackSide::Invalid => serializer.serialize_str(Self::INVALID_STR),
            BackSide::Time(ref t) => serializer.serialize_str(&t.serialize()),
            BackSide::Text(ref s) => serializer.serialize_str(s),
            BackSide::Card(ref id) => serializer.serialize_str(&id.to_string()),
            BackSide::List(ref ids) => {
                let mut seq = serializer.serialize_seq(Some(ids.len()))?;
                for id in ids {
                    seq.serialize_element(&id.to_string())?;
                }
                seq.end()
            }
        }
    }
}

fn is_false(flag: &bool) -> bool {
    !flag
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct RawType {
    pub ty: CType,
    pub front: Option<String>,
    pub back: Option<BackSide>,
    pub class: Option<Uuid>,
    pub instance: Option<Uuid>,
    pub attribute: Option<Uuid>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub parent_event: Option<Uuid>,
}

impl RawType {
    pub fn class(&self) -> Option<Uuid> {
        self.class.clone()
    }
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct RawCard {
    pub id: Uuid,
    #[serde(flatten)]
    pub data: RawType,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub dependencies: BTreeSet<Uuid>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tags: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub suspended: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub deleted: bool,
    #[serde(default)]
    pub last_modified: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttributeDTO {
    pub pattern: String,
    pub id: AttributeId,
    pub class: CardId,
    pub back_type: Option<CardId>,
    #[serde(default)]
    pub last_modified: Duration,
    #[serde(default)]
    pub deleted: bool,
}

pub type AttributeId = Uuid;

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
