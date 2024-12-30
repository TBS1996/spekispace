use async_trait::async_trait;
use omtrent::TimeStamp;
use serde::{ser::SerializeSeq, Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Debug;
use std::str::FromStr;
use std::time::Duration;
use tracing::info;
use uuid::Uuid;

#[derive(Clone, Copy)]
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

    fn identifier(&self) -> &'static str {
        "reviews"
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
            MergeRes::Other(merged)
        } else if mergedlen == otherlen {
            MergeRes::Selv(merged)
        } else {
            MergeRes::Both(merged)
        }
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

    fn identifier(&self) -> &'static str {
        "attributes"
    }
}

impl Item for RawCard {
    fn last_modified(&self) -> Duration {
        self.last_modified
    }

    fn id(&self) -> Uuid {
        self.id
    }

    fn serialize(&self) -> String {
        toml::to_string(self).unwrap()
    }

    fn identifier(&self) -> &'static str {
        "cards"
    }

    fn merge(self, other: Self) -> MergeRes<Self>
    where
        Self: Sized,
    {
        let selfmod = self.last_modified();
        let othermod = other.last_modified();

        if self.deleted && !other.deleted {
            MergeRes::Other(self)
        } else if !self.deleted && other.deleted {
            MergeRes::Selv(other)
        } else if selfmod > othermod {
            MergeRes::Other(self)
        } else if selfmod < othermod {
            MergeRes::Selv(other)
        } else {
            MergeRes::Neither
        }
    }
}

trait Item {
    fn last_modified(&self) -> Duration;
    fn id(&self) -> Uuid;
    fn serialize(&self) -> String;
    fn identifier(&self) -> &'static str;
    /// Returns whehter hte returned value should be saved to self, other, enither, or both.
    fn merge(self, other: Self) -> MergeRes<Self>
    where
        Self: Sized,
    {
        if self.last_modified() > other.last_modified() {
            MergeRes::Other(self)
        } else if self.last_modified() < other.last_modified() {
            MergeRes::Selv(other)
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

enum MergeRes<T> {
    Selv(T),
    Other(T),
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

#[async_trait(?Send)]
pub trait SpekiProvider: Sync {
    async fn load_record(&self, id: Uuid, ty: Cty) -> Option<Record>;
    async fn load_all_records(&self, ty: Cty) -> HashMap<Uuid, Record>;

    async fn load_content(&self, id: Uuid, ty: Cty) -> Option<String> {
        self.load_record(id, ty).await.map(|rec| rec.content)
    }

    async fn last_modified(&self, id: Uuid, ty: Cty) -> Option<Duration> {
        self.load_record(id, ty)
            .await
            .map(|rec| Duration::from_secs(rec.last_modified))
    }

    async fn load_all_content(&self, ty: Cty) -> Vec<String> {
        self.load_all_records(ty)
            .await
            .into_values()
            .map(|rec| rec.content)
            .collect()
    }

    async fn save_content(&self, ty: Cty, record: Record);

    async fn delete_content(&self, id: Uuid, ty: Cty);

    async fn load_all_reviews(&self) -> HashMap<Uuid, History> {
        self.load_all_records(Cty::Review)
            .await
            .into_iter()
            .map(|rev| (rev.0, parse_history(rev.1.content, rev.0)))
            .collect()
    }

    async fn load_all_cards(&self) -> Vec<RawCard> {
        self.load_all_content(Cty::Card)
            .await
            .into_iter()
            .filter_map(|content| {
                let card = toml::from_str::<RawCard>(&content).unwrap();
                if !card.deleted {
                    Some(card)
                } else {
                    None
                }
            })
            .collect()
    }

    async fn save_card(&self, card: RawCard) {
        let record = card.into_record();
        self.save_content(Cty::Card, record).await;
    }

    async fn load_card(&self, id: CardId) -> Option<RawCard> {
        let content = self.load_content(id, Cty::Card).await?;

        let card = toml::from_str::<RawCard>(&content).unwrap();

        if card.deleted {
            None
        } else {
            Some(card)
        }
    }

    async fn delete_card(&self, id: CardId) {
        self.delete_content(id, Cty::Card).await;
    }

    async fn load_all_attributes(&self) -> Vec<AttributeDTO> {
        self.load_all_content(Cty::Attribute)
            .await
            .into_iter()
            .map(|content| toml::from_str(&content).unwrap())
            .collect()
    }

    async fn save_attribute(&self, attribute: AttributeDTO) {
        self.save_content(Cty::Attribute, attribute.into_record())
            .await;
    }

    async fn load_attribute(&self, id: AttributeId) -> Option<AttributeDTO> {
        self.load_content(id, Cty::Attribute)
            .await
            .map(|card| toml::from_str(&card).unwrap())
    }

    async fn delete_attribute(&self, id: AttributeId) {
        self.delete_content(id, Cty::Attribute).await;
    }

    async fn load_reviews(&self, id: CardId) -> History {
        let Some(s) = self.load_content(id, Cty::Review).await else {
            return History::new(id);
        };

        parse_history(s, id)
    }

    async fn save_reviews(&self, reviews: History) {
        self.save_content(Cty::Review, reviews.into_record()).await;
    }

    async fn load_config(&self) -> Config;
    async fn save_config(&self, config: Config);

    async fn last_modified_card(&self, id: CardId) -> Duration {
        self.last_modified(id, Cty::Card).await.unwrap_or_default()
    }

    async fn last_modified_attribute(&self, id: AttributeId) -> Duration {
        self.last_modified(id, Cty::Attribute)
            .await
            .unwrap_or_default()
    }

    async fn last_modified_reviews(&self, id: CardId) -> Option<Duration> {
        self.last_modified(id, Cty::Review).await
    }
    async fn load_card_ids(&self) -> Vec<CardId> {
        self.load_all_cards()
            .await
            .into_iter()
            .filter_map(|raw| {
                info!("loading raw: {raw:?}");

                if !raw.deleted {
                    Some(raw.id)
                } else {
                    None
                }
            })
            .collect()
    }

    async fn add_review(&self, id: CardId, review: Review) {
        let mut reviews = self.load_reviews(id).await;
        reviews.push(review);
        self.save_reviews(reviews).await;
    }

    async fn sync_cards(&self, other: &Box<dyn SpekiProvider>) -> HashSet<CardId> {
        info!("loading cards 1");
        let mut self_update = vec![];
        let mut other_update = vec![];

        let mut map1: HashMap<CardId, RawCard> = self
            .load_all_content(Cty::Card)
            .await
            .into_iter()
            .map(|card| {
                let card: RawCard = toml::from_str(&card).unwrap();

                (card.id, card)
            })
            .collect();

        info!("loading cards 2");
        let mut map2: HashMap<CardId, RawCard> = other
            .load_all_content(Cty::Card)
            .await
            .into_iter()
            .map(|card| {
                let card: RawCard = toml::from_str(&card).unwrap();

                (card.id, card)
            })
            .collect();

        let mut ids: HashSet<CardId> = map1.keys().map(|key| *key).collect();
        ids.extend(map2.keys());

        for id in &ids {
            info!("syncing card");
            let id = *id;
            match (map1.remove(&id), map2.remove(&id)) {
                (None, None) => panic!(),
                (None, Some(card)) => {
                    self_update.push(card);
                }
                (Some(card), None) => {
                    other_update.push(card);
                }
                (Some(card1), Some(card2)) => match card1.merge(card2) {
                    MergeRes::Both(card) => {
                        self_update.push(card.clone());
                        other_update.push(card);
                    }
                    MergeRes::Selv(card) => {
                        self_update.push(card);
                    }
                    MergeRes::Other(card) => {
                        other_update.push(card);
                    }
                    MergeRes::Neither => {}
                },
            }
        }

        for card in self_update {
            self.save_card(card).await;
        }

        for card in other_update {
            other.save_card(card).await;
        }

        ids
    }

    async fn sync_reviews(&self, other: &Box<dyn SpekiProvider>, ids: HashSet<CardId>) {
        info!("loading self reviews");
        let mut selfmap: HashMap<Uuid, History> = self.load_all_reviews().await;
        info!("loading other reviews");
        let mut othermap: HashMap<Uuid, History> = other.load_all_reviews().await;

        let mut self_update = vec![];
        let mut other_update = vec![];

        for id in &ids {
            info!("syncing review");
            let id = *id;

            match (selfmap.remove(&id), othermap.remove(&id)) {
                (None, None) => continue,
                (None, Some(rev2)) => {
                    self_update.push(rev2);
                }
                (Some(rev1), None) => {
                    other_update.push(rev1);
                }
                (Some(rev1), Some(rev2)) => match rev1.merge(rev2) {
                    MergeRes::Both(rev) => {
                        self_update.push(rev.clone());
                        other_update.push(rev);
                    }
                    MergeRes::Selv(rev) => {
                        self_update.push(rev);
                    }
                    MergeRes::Other(rev) => {
                        other_update.push(rev);
                    }
                    MergeRes::Neither => {}
                },
            }
        }

        for rev in self_update {
            self.save_reviews(rev).await;
        }

        for rev in other_update {
            other.save_reviews(rev).await;
        }
    }

    async fn sync_attributes(&self, other: &Box<dyn SpekiProvider>) {
        let mut self_update = vec![];
        let mut other_update = vec![];

        info!("fetching attributes 1");
        let mut selfmap: HashMap<AttributeId, AttributeDTO> = self
            .load_all_attributes()
            .await
            .into_iter()
            .map(|card| (card.id, card))
            .collect();

        info!("fetching attributes 2");
        let mut othermap: HashMap<AttributeId, AttributeDTO> = other
            .load_all_attributes()
            .await
            .into_iter()
            .map(|card| (card.id, card))
            .collect();

        let mut ids: HashSet<AttributeId> = selfmap.keys().map(|key| *key).collect();
        ids.extend(othermap.keys());

        for id in ids {
            info!("syncing attribute");
            match (selfmap.remove(&id), othermap.remove(&id)) {
                (None, None) => panic!(),
                (None, Some(card)) => {
                    self_update.push(card);
                }
                (Some(card), None) => {
                    other_update.push(card);
                }
                (Some(card1), Some(card2)) => match card1.merge(card2) {
                    MergeRes::Both(rev) => {
                        self_update.push(rev.clone());
                        other_update.push(rev);
                    }
                    MergeRes::Selv(rev) => {
                        self_update.push(rev);
                    }
                    MergeRes::Other(rev) => {
                        other_update.push(rev);
                    }
                    MergeRes::Neither => {}
                },
            }
        }

        for rev in self_update {
            self.save_attribute(rev).await;
        }

        for rev in other_update {
            other.save_attribute(rev).await;
        }
    }

    async fn sync(&self, other: &Box<dyn SpekiProvider>) {
        let ids = self.sync_cards(other).await;
        self.sync_reviews(other, ids).await;
        self.sync_attributes(other).await;
    }
}

fn parse_history(s: String, id: Uuid) -> History {
    if let Ok(history) = toml::from_str(&s) {
        history
    } else {
        History {
            id,
            reviews: parse_review(s),
        }
    }
}

fn parse_review(s: String) -> Vec<Review> {
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
