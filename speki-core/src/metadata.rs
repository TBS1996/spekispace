use std::time::Duration;

use ledgerstore::LedgerItem;
use serde::{
    de::{self, Deserializer},
    Deserialize, Serialize,
};
use uuid::Uuid;

use crate::{card::CardId, ledger::MetaAction};

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_not_suspended(val: &IsSuspended) -> bool {
    !val.is_suspended()
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Hash, PartialEq, Eq)]
pub struct Metadata {
    #[serde(default, skip_serializing_if = "is_false")]
    pub needs_work: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trivial: Option<bool>,
    #[serde(default, skip_serializing_if = "is_not_suspended")]
    pub suspended: IsSuspended,
    pub id: Uuid,
}

impl Metadata {
    pub fn new(id: CardId) -> Self {
        Self {
            id,
            trivial: None,
            needs_work: false,
            suspended: Default::default(),
        }
    }
}

impl LedgerItem for Metadata {
    type Error = ();
    type Key = Uuid;
    type PropertyType = String;
    type RefType = String;
    type Modifier = MetaAction;

    fn inner_run_event(mut self, event: MetaAction) -> Result<Self, ()> {
        match event {
            crate::ledger::MetaAction::Suspend(flag) => self.suspended = flag.into(),
            crate::ledger::MetaAction::SetTrivial(flag) => self.trivial = flag,
            crate::ledger::MetaAction::SetNeedsWork(flag) => self.needs_work = flag,
        }

        Ok(self)
    }

    fn new_default(id: CardId) -> Self {
        Self::new(id)
    }

    fn item_id(&self) -> CardId {
        self.id
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
