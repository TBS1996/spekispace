use async_trait::async_trait;
use core::fmt;
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

    async fn load_all_reviews(&self) -> HashMap<Uuid, Vec<Review>> {
        self.load_all_records(Cty::Review)
            .await
            .into_iter()
            .map(|rev| (rev.0, parse_review(rev.1.content)))
            .collect()
    }

    async fn load_all_cards(&self) -> Vec<RawCard> {
        self.load_all_content(Cty::Card)
            .await
            .into_iter()
            .map(|content| toml::from_str(&content).unwrap())
            .collect()
    }

    async fn save_card(&self, card: RawCard) {
        self.save_content(Cty::Card, card.id, toml::to_string(&card).unwrap())
            .await;
    }

    async fn load_card(&self, id: CardId) -> Option<RawCard> {
        self.load_content(id.into_inner(), Cty::Card)
            .await
            .map(|card| toml::from_str(&card).unwrap())
    }

    async fn delete_card(&self, id: CardId) {
        self.delete_content(id.into_inner(), Cty::Card).await;
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
            attribute.id.into_inner(),
            toml::to_string(&attribute).unwrap(),
        )
        .await;
    }

    async fn load_attribute(&self, id: AttributeId) -> Option<AttributeDTO> {
        self.load_content(id.into_inner(), Cty::Attribute)
            .await
            .map(|card| toml::from_str(&card).unwrap())
    }

    async fn delete_attribute(&self, id: AttributeId) {
        self.delete_content(id.into_inner(), Cty::Attribute).await;
    }

    async fn load_reviews(&self, id: CardId) -> Vec<Review> {
        let Some(s) = self.load_content(id.into_inner(), Cty::Review).await else {
            return vec![];
        };

        parse_review(s)
    }

    async fn save_reviews(&self, id: CardId, reviews: Vec<Review>) {
        let mut s = String::new();
        for r in reviews {
            let stamp = r.timestamp.as_secs().to_string();
            let grade = match r.grade {
                Recall::None => "1",
                Recall::Late => "2",
                Recall::Some => "3",
                Recall::Perfect => "4",
            };
            s.push_str(&format!("{} {}\n", stamp, grade));
        }

        self.save_content(Cty::Review, id.into_inner(), s).await;
    }

    async fn load_config(&self) -> Config;
    async fn save_config(&self, config: Config);

    async fn last_modified_card(&self, id: CardId) -> Duration {
        self.last_modified(id.into_inner(), Cty::Card)
            .await
            .unwrap_or_default()
    }

    async fn last_modified_attribute(&self, id: AttributeId) -> Duration {
        self.last_modified(id.into_inner(), Cty::Attribute)
            .await
            .unwrap_or_default()
    }

    async fn last_modified_reviews(&self, id: CardId) -> Option<Duration> {
        self.last_modified(id.into_inner(), Cty::Review).await
    }
    async fn load_card_ids(&self) -> Vec<CardId> {
        self.load_all_cards()
            .await
            .into_iter()
            .map(|raw| CardId(raw.id))
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
            .load_all_cards()
            .await
            .into_iter()
            .map(|card| (CardId(card.id), card))
            .collect();

        info!("loading cards 2");
        let mut map2: HashMap<CardId, RawCard> = other
            .load_all_cards()
            .await
            .into_iter()
            .map(|card| (CardId(card.id), card))
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
                (Some(card1), Some(card2)) => {
                    let lm1 = self.last_modified_card(CardId(card1.id)).await;
                    let lm2 = self.last_modified_card(CardId(card2.id)).await;

                    if lm1 > lm2 {
                        self.save_card(card2).await;
                    } else if lm1 < lm2 {
                        other.save_card(card1).await;
                    }
                }
            }
        }

        info!("REVIEW SYNCING");

        info!("loading self reviews");
        let mut map1: HashMap<Uuid, Vec<Review>> = self.load_all_reviews().await;
        info!("loading other reviews");
        let mut map2: HashMap<Uuid, Vec<Review>> = other.load_all_reviews().await;

        for id in &ids {
            info!("syncing review");
            let id = *id;

            match (map1.remove(&id.into_inner()), map2.remove(&id.into_inner())) {
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
                        cmb.extend(rev2);
                        cmb.dedup();
                        cmb.sort_by_key(|rev| rev.timestamp);
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

impl fmt::Display for AttributeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
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
