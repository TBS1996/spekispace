use std::time::Duration;

use serde::{
    de::{self, Deserializer},
    Deserialize, Serialize,
};
use speki_dto::{RunLedger};
use uuid::Uuid;

use crate::{card::CardId, ledger::{MetaAction, MetaEvent}};

#[derive(Clone, Debug, Default, Deserialize, Serialize, Hash)]
pub struct Metadata {
    pub suspended: IsSuspended,
    id: Uuid,
}

impl Metadata {
    pub fn new(id: CardId) -> Self {
        Self {
            id,
            suspended: Default::default(),
        }
    }
}

impl RunLedger<MetaEvent> for Metadata {
    fn run_event(mut self, event: MetaEvent) -> Self {
        match event.action {
            crate::ledger::MetaAction::Suspend(flag) => self.suspended = flag.into(),
        }

        self
    }

    fn derive_events(&self) -> Vec<MetaEvent> {
        let mut actions: Vec<MetaEvent> = vec![];

        if self.suspended.is_suspended() {
            actions.push(MetaEvent {
                id: self.id,
                action: MetaAction::Suspend(true)
            });
        }

        actions
    }
    
    fn new_default(id: String) -> Self {
        Self::new(id.parse().unwrap())
    }
    
    fn item_id(&self) -> String {
        self.id.to_string()
    }
    
    fn identifier() -> &'static str {
        "metadata"
    }
}

#[derive(Ord, PartialOrd, Eq, PartialEq, Hash, Debug, Clone)]
pub enum IsSuspended {
    False,
    True,
    // Card is temporarily suspended, until contained unix time has passed.
    TrueUntil(Duration),
}

impl From<bool> for IsSuspended {
    fn from(value: bool) -> Self {
        match value {
            true => Self::True,
            false => Self::False,
        }
    }
}

impl Default for IsSuspended {
    fn default() -> Self {
        Self::False
    }
}

impl IsSuspended {
    fn verify_time(self, current_time: Duration) -> Self {
        if let Self::TrueUntil(dur) = self {
            if dur < current_time {
                return Self::False;
            }
        }
        self
    }

    pub fn is_suspended(&self) -> bool {
        !matches!(self, IsSuspended::False)
    }
}

impl Serialize for IsSuspended {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        match self.clone().verify_time(Duration::default()) {
            IsSuspended::False => serializer.serialize_bool(false),
            IsSuspended::True => serializer.serialize_bool(true),
            IsSuspended::TrueUntil(duration) => serializer.serialize_u64(duration.as_secs()),
        }
    }
}

impl<'de> Deserialize<'de> for IsSuspended {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value: toml::Value = Deserialize::deserialize(deserializer)?;

        match value {
            toml::Value::Boolean(b) => Ok(b.into()),
            toml::Value::Integer(i) => {
                if let Ok(secs) = std::convert::TryInto::<u64>::try_into(i) {
                    Ok(IsSuspended::TrueUntil(Duration::from_secs(secs))
                        .verify_time(Duration::default()))
                } else {
                    Err(de::Error::custom("Invalid duration format"))
                }
            }

            _ => Err(serde::de::Error::custom("Invalid value for IsDisabled")),
        }
    }
}
