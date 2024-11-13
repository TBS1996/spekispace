use async_trait::async_trait;
use core::fmt;
use omtrent::TimeStamp;
use serde::{ser::SerializeSeq, Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;
use std::str::FromStr;
use std::time::Duration;
use uuid::Uuid;

use futures::executor::block_on;

#[async_trait(?Send)]
pub trait SpekiProvider: Sync {
    async fn load_all_cards(&self) -> Vec<RawCard>;
    async fn save_card(&self, card: RawCard);
    async fn load_card(&self, id: CardId) -> Option<RawCard>;
    async fn delete_card(&self, id: CardId);
    async fn load_all_attributes(&self) -> Vec<AttributeDTO>;
    async fn save_attribute(&self, attribute: AttributeDTO);
    async fn load_attribute(&self, id: AttributeId) -> Option<AttributeDTO>;
    async fn delete_attribute(&self, id: AttributeId);
    async fn load_reviews(&self, id: CardId) -> Vec<Review>;
    async fn save_reviews(&self, id: CardId, reviews: Vec<Review>);
    async fn load_config(&self) -> Config;
    async fn save_config(&self, config: Config);

    fn blocking_load_all_cards(&self) -> Vec<RawCard> {
        block_on(self.load_all_cards())
    }

    fn blocking_save_card(&self, card: RawCard) {
        block_on(self.save_card(card))
    }

    fn blocking_load_card(&self, id: CardId) -> Option<RawCard> {
        block_on(self.load_card(id))
    }

    fn blocking_delete_card(&self, id: CardId) {
        block_on(self.delete_card(id))
    }

    fn blocking_load_all_attributes(&self) -> Vec<AttributeDTO> {
        block_on(self.load_all_attributes())
    }

    fn blocking_save_attribute(&self, attribute: AttributeDTO) {
        block_on(self.save_attribute(attribute))
    }

    fn blocking_load_attribute(&self, id: AttributeId) -> Option<AttributeDTO> {
        block_on(self.load_attribute(id))
    }

    fn blocking_delete_attribute(&self, id: AttributeId) {
        block_on(self.delete_attribute(id))
    }

    fn blocking_load_reviews(&self, id: CardId) -> Vec<Review> {
        block_on(self.load_reviews(id))
    }

    fn blocking_save_reviews(&self, id: CardId, reviews: Vec<Review>) {
        block_on(self.save_reviews(id, reviews))
    }

    fn blocking_load_config(&self) -> Config {
        block_on(self.load_config())
    }

    fn blocking_save_config(&self, config: Config) {
        block_on(self.save_config(config))
    }

    async fn add_review(&self, id: CardId, review: Review) {
        let mut reviews = self.load_reviews(id).await;
        let foo = reviews.len();
        reviews.push(review);
        self.save_reviews(id, reviews).await;
        let bar = self.load_reviews(id).await.len();
        assert!(foo < bar);
    }

    fn blocking_add_review(&self, id: CardId, review: Review) {
        let mut reviews = self.blocking_load_reviews(id);
        let foo = reviews.len();
        reviews.push(review);
        self.blocking_save_reviews(id, reviews);
        let bar = self.blocking_load_reviews(id).len();
        assert!(foo < bar);
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Config;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
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

#[derive(Serialize, Deserialize, Debug, Clone, Ord, Eq, PartialEq, PartialOrd, Copy, Hash)]
#[serde(transparent)]
pub struct CardId(pub Uuid);

impl FromStr for CardId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::from_str(s).map(CardId)
    }
}

impl AsRef<Uuid> for CardId {
    fn as_ref(&self) -> &Uuid {
        &self.0
    }
}

impl CardId {
    pub fn into_inner(self) -> Uuid {
        self.0
    }
}

impl fmt::Display for CardId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Ord, PartialOrd, Eq, Hash, PartialEq, Debug, Clone)]
pub enum BackSide {
    Text(String),
    Card(CardId),
    List(Vec<CardId>),
    Time(TimeStamp),
    Trivial, // Answer is obvious, used when card is more of a dependency anchor
}

impl Default for BackSide {
    fn default() -> Self {
        Self::Text(Default::default())
    }
}

impl From<String> for BackSide {
    fn from(s: String) -> Self {
        if let Ok(uuid) = Uuid::parse_str(&s) {
            BackSide::Card(CardId(uuid))
        } else if let Some(timestamp) = TimeStamp::from_string(s.clone()) {
            BackSide::Time(timestamp)
        } else {
            BackSide::Text(s)
        }
    }
}

impl BackSide {
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
                            ids.push(CardId(uuid));
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
            BackSide::Time(ref t) => serializer.serialize_str(&t.serialize()),
            BackSide::Text(ref s) => serializer.serialize_str(s),
            BackSide::Card(ref id) => serializer.serialize_str(&id.0.to_string()),
            BackSide::List(ref ids) => {
                let mut seq = serializer.serialize_seq(Some(ids.len()))?;
                for id in ids {
                    seq.serialize_element(&id.0.to_string())?;
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

#[derive(Serialize, Deserialize, Default, Debug)]
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttributeDTO {
    pub pattern: String,
    pub id: AttributeId,
    pub class: CardId,
    pub back_type: Option<CardId>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Ord, Eq, PartialEq, PartialOrd, Copy, Hash)]
#[serde(transparent)]
pub struct AttributeId(pub Uuid);

impl AsRef<Uuid> for AttributeId {
    fn as_ref(&self) -> &Uuid {
        &self.0
    }
}

impl AttributeId {
    pub fn into_inner(self) -> Uuid {
        self.0
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
