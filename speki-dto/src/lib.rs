use core::fmt;
use omtrent::TimeStamp;
use serde::{ser::SerializeSeq, Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fmt::Debug;
use std::str::FromStr;
use uuid::Uuid;

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
/*
pub fn matches_constraint(&self, constraint: BackConstraint) -> bool {
    match (self, constraint) {
        (BackSide::Card(card_id), BackConstraint::Card(card_characteristic)) => {
            card_characteristic.card_matches(*card_id)
        }
        (BackSide::List(cardlist), BackConstraint::List(vec)) => {
            if cardlist.len() != vec.len() {
                return false;
            }
            cardlist
                .iter()
                .zip(vec.iter())
                .all(|(card, charac)| charac.card_matches(*card))
        }

        (BackSide::Time(_), BackConstraint::Time) => true,

        (_, _) => false,
    }
}
*/

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
