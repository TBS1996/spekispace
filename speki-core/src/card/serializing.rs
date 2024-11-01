use crate::attribute::AttributeId;
use crate::paths;
use filecash::FsLoad;
use omtrent::TimeStamp;
use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};
use speki_dto::CardId;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;
use std::path::PathBuf;
use std::time::Duration;
use toml::Value;
use uuid::Uuid;

use speki_dto::{CType, RawType};

use super::{
    AnyType, AttributeCard, Card, CardTrait, ClassCard, EventCard, InstanceCard, IsSuspended,
    NormalCard, StatementCard, UnfinishedCard,
};

fn is_false(flag: &bool) -> bool {
    !flag
}

pub fn into_any(raw: RawType) -> AnyType {
    match raw.ty {
        CType::Instance => InstanceCard {
            name: raw.front.unwrap(),
            class: raw.class.map(CardId).unwrap(),
            back: raw.back,
        }
        .into(),
        CType::Normal => NormalCard {
            front: raw.front.unwrap(),
            back: raw.back.unwrap(),
        }
        .into(),
        CType::Unfinished => UnfinishedCard {
            front: raw.front.unwrap(),
        }
        .into(),
        CType::Attribute => AttributeCard {
            attribute: AttributeId::verify(raw.attribute.unwrap()).unwrap(),
            back: raw.back.unwrap(),
            instance: CardId(raw.instance.unwrap()),
        }
        .into(),
        CType::Class => ClassCard {
            name: raw.front.unwrap(),
            back: raw.back.unwrap(),
            parent_class: raw.class.map(CardId),
        }
        .into(),
        CType::Statement => StatementCard {
            front: raw.front.unwrap(),
        }
        .into(),
        CType::Event => EventCard {
            front: raw.front.unwrap(),
            start_time: raw
                .start_time
                .clone()
                .map(TimeStamp::from_string)
                .flatten()
                .unwrap_or_default(),
            end_time: raw.end_time.clone().map(TimeStamp::from_string).flatten(),
            parent_event: raw.parent_event.map(CardId),
        }
        .into(),
    }
}

pub fn from_any(ty: AnyType) -> RawType {
    let mut raw = RawType::default();
    let fieldless = ty.fieldless();
    raw.ty = fieldless;

    match ty {
        AnyType::Instance(InstanceCard { name, class, back }) => {
            raw.class = Some(class.into_inner());
            raw.front = Some(name);
            raw.back = back;
        }
        AnyType::Normal(NormalCard { front, back }) => {
            raw.front = Some(front);
            raw.back = Some(back);
        }
        AnyType::Unfinished(UnfinishedCard { front }) => {
            raw.front = Some(front);
        }
        AnyType::Attribute(AttributeCard {
            attribute,
            back,
            instance,
        }) => {
            raw.attribute = Some(attribute.into_inner());
            raw.back = Some(back);
            raw.instance = Some(instance.into_inner());
        }
        AnyType::Class(ClassCard {
            name,
            back,
            parent_class,
        }) => {
            raw.front = Some(name);
            raw.back = Some(back);
            raw.class = parent_class.map(CardId::into_inner);
        }
        AnyType::Statement(StatementCard { front }) => {
            raw.front = Some(front);
        }
        AnyType::Event(EventCard {
            front,
            start_time,
            end_time,
            parent_event,
        }) => {
            raw.front = Some(front);
            raw.start_time = Some(start_time.serialize());
            raw.end_time = end_time.map(|t| t.serialize());
            raw.parent_event = parent_event.map(CardId::into_inner);
        }
    };

    raw
}

impl FsLoad for RawCard {
    fn id(&self) -> Uuid {
        self.id
    }

    fn type_name() -> String {
        String::from("speki")
    }

    fn save_paths() -> Vec<PathBuf> {
        let p1 = paths::get_cards_path();
        let p2 = paths::get_collections_path();
        vec![p1, p2]
    }

    fn file_name(&self) -> String {
        into_any(self.data.clone()).display_front()
    }

    fn dependencies(&self) -> BTreeSet<Uuid> {
        let mut deps = self.dependencies.clone();
        let _any = into_any(self.data.clone());
        let other_deps: BTreeSet<Uuid> = _any
            .get_dependencies()
            .into_iter()
            .map(|id| id.into_inner())
            .collect();
        deps.extend(other_deps.iter());

        deps
    }
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

impl RawCard {
    pub fn new(card: impl Into<AnyType>) -> Self {
        let card: AnyType = card.into();
        match card {
            AnyType::Instance(concept) => Self::new_concept(concept),
            AnyType::Normal(normal) => Self::new_normal(normal),
            AnyType::Unfinished(unfinished) => Self::new_unfinished(unfinished),
            AnyType::Attribute(attribute) => Self::new_attribute(attribute),
            AnyType::Class(class) => Self::new_class(class),
            AnyType::Statement(statement) => Self::new_statement(statement),
            AnyType::Event(event) => Self::new_event(event),
        }
    }

    pub fn new_unfinished(unfinished: UnfinishedCard) -> Self {
        Self {
            id: Uuid::new_v4(),
            data: from_any(unfinished.into()),
            ..Default::default()
        }
    }

    pub fn new_event(statement: EventCard) -> Self {
        Self {
            id: Uuid::new_v4(),
            data: from_any(statement.into()),
            ..Default::default()
        }
    }

    pub fn new_statement(statement: StatementCard) -> Self {
        Self {
            id: Uuid::new_v4(),
            data: from_any(statement.into()),
            ..Default::default()
        }
    }

    pub fn new_class(class: ClassCard) -> Self {
        Self {
            id: Uuid::new_v4(),
            data: from_any(class.into()),
            ..Default::default()
        }
    }
    pub fn new_attribute(attr: AttributeCard) -> Self {
        Self {
            id: Uuid::new_v4(),
            data: from_any(attr.into()),
            ..Default::default()
        }
    }
    pub fn new_concept(concept: InstanceCard) -> Self {
        Self {
            id: Uuid::new_v4(),
            data: from_any(concept.into()),
            ..Default::default()
        }
    }
    pub fn new_normal(normal: NormalCard) -> Self {
        Self {
            id: Uuid::new_v4(),
            data: from_any(normal.into()),
            ..Default::default()
        }
    }

    pub fn from_card(card: Card<AnyType>) -> Self {
        Self {
            id: card.id.into_inner(),
            data: from_any(card.data),
            dependencies: card
                .dependencies
                .into_iter()
                .map(|id| id.into_inner())
                .collect(),
            tags: card.tags,
            suspended: card.suspended.is_suspended(),
        }
    }
}

impl Serialize for IsSuspended {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        match self.clone().verify_time() {
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
        let value: Value = Deserialize::deserialize(deserializer)?;

        match value {
            Value::Boolean(b) => Ok(b.into()),
            Value::Integer(i) => {
                if let Ok(secs) = std::convert::TryInto::<u64>::try_into(i) {
                    Ok(IsSuspended::TrueUntil(Duration::from_secs(secs)).verify_time())
                } else {
                    Err(de::Error::custom("Invalid duration format"))
                }
            }

            _ => Err(serde::de::Error::custom("Invalid value for IsDisabled")),
        }
    }
}
