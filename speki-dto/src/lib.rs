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

    async fn save_content(&self, ty: Cty, id: Uuid, content: String);

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
        self.save_content(Cty::Card, card.id, toml::to_string(&card).unwrap())
            .await;
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
        self.save_content(
            Cty::Attribute,
            attribute.id,
            toml::to_string(&attribute).unwrap(),
        )
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

    async fn save_reviews(&self, id: CardId, reviews: History) {
        let s = toml::to_string(&reviews).unwrap();
        self.save_content(Cty::Review, id, s).await;
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
        self.save_reviews(id, reviews).await;
    }

    async fn sync(&self, other: Box<dyn SpekiProvider>) {
        info!("loading cards 1");
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
                (None, Some(card)) => self.save_card(card).await,
                (Some(card), None) => other.save_card(card).await,
                (Some(mut card1), Some(mut card2)) => {
                    let lm1 = self.last_modified_card(card1.id).await;
                    let lm2 = self.last_modified_card(card2.id).await;

                    if card1.deleted && !card2.deleted {
                        card2.deleted = true;
                        self.save_card(card2).await;
                    } else if !card1.deleted && card2.deleted {
                        card1.deleted = true;
                        other.save_card(card1).await;
                    } else if lm1 > lm2 {
                        self.save_card(card2).await;
                    } else if lm1 < lm2 {
                        other.save_card(card1).await;
                    }
                }
            }
        }

        info!("REVIEW SYNCING");

        info!("loading self reviews");
        let mut map1: HashMap<Uuid, History> = self.load_all_reviews().await;
        info!("loading other reviews");
        let mut map2: HashMap<Uuid, History> = other.load_all_reviews().await;

        for id in &ids {
            info!("syncing review");
            let id = *id;

            match (map1.remove(&id), map2.remove(&id)) {
                (None, None) => continue,
                (None, Some(rev2)) => self.save_reviews(id, rev2).await,
                (Some(rev1), None) => other.save_reviews(id, rev1).await,
                (Some(rev1), Some(rev2)) => {
                    let rev1_qty = rev1.len();
                    let rev2_qty = rev2.len();

                    if rev1_qty == rev2_qty {
                        continue;
                    }

                    let combined = {
                        let mut cmb = rev1.clone();
                        cmb.merge_into(rev2);
                        cmb
                    };

                    let comb_qty = combined.len();

                    if comb_qty > rev1_qty {
                        info!("save reviews to self");
                        self.save_reviews(id, combined.clone()).await;
                    }

                    if comb_qty > rev1_qty {
                        info!("save reviews to other");
                        other.save_reviews(id, combined.clone()).await;
                    }
                }
            }
        }

        info!("fetching attributes 1");
        let mut map1: HashMap<AttributeId, AttributeDTO> = self
            .load_all_attributes()
            .await
            .into_iter()
            .map(|card| (card.id, card))
            .collect();

        info!("fetching attributes 2");
        let mut map2: HashMap<AttributeId, AttributeDTO> = other
            .load_all_attributes()
            .await
            .into_iter()
            .map(|card| (card.id, card))
            .collect();

        let mut ids: HashSet<AttributeId> = map1.keys().map(|key| *key).collect();
        ids.extend(map2.keys());

        for id in ids {
            info!("syncing attribute");
            match (map1.remove(&id), map2.remove(&id)) {
                (None, None) => panic!(),
                (None, Some(card)) => self.save_attribute(card).await,
                (Some(card), None) => other.save_attribute(card).await,
                (Some(card1), Some(card2)) => {
                    let lm1 = self.last_modified_attribute(card1.id).await;
                    let lm2 = self.last_modified_attribute(card2.id).await;

                    if lm1 > lm2 {
                        self.save_attribute(card2).await;
                    } else if lm1 < lm2 {
                        other.save_attribute(card1).await;
                    }
                }
            }
        }
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttributeDTO {
    pub pattern: String,
    pub id: AttributeId,
    pub class: CardId,
    pub back_type: Option<CardId>,
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
