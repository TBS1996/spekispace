use std::time::Duration;

use omtrent::TimeStamp;
use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};
use speki_dto::RawCard;
use speki_dto::{AttributeId, CardId};
use speki_dto::{CType, RawType};
use toml::Value;
use uuid::Uuid;

use super::{
    CardType, AttributeCard, Card, ClassCard, EventCard, InstanceCard, IsSuspended, NormalCard,
    StatementCard, UnfinishedCard,
};
use crate::card_provider::CardProvider;

pub fn into_any(raw: RawType, card_provider: &CardProvider) -> CardType {
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
            attribute: AttributeId(raw.attribute.unwrap()),
            back: raw.back.unwrap(),
            instance: CardId(raw.instance.unwrap()),
            card_provider: card_provider.clone(),
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

pub fn from_any(ty: CardType) -> RawType {
    let mut raw = RawType::default();
    let fieldless = ty.fieldless();
    raw.ty = fieldless;

    match ty {
        CardType::Instance(InstanceCard { name, class, back }) => {
            raw.class = Some(class.into_inner());
            raw.front = Some(name);
            raw.back = back;
        }
        CardType::Normal(NormalCard { front, back }) => {
            raw.front = Some(front);
            raw.back = Some(back);
        }
        CardType::Unfinished(UnfinishedCard { front }) => {
            raw.front = Some(front);
        }
        CardType::Attribute(AttributeCard {
            attribute,
            back,
            instance,
            card_provider: _,
        }) => {
            raw.attribute = Some(attribute.into_inner());
            raw.back = Some(back);
            raw.instance = Some(instance.into_inner());
        }
        CardType::Class(ClassCard {
            name,
            back,
            parent_class,
        }) => {
            raw.front = Some(name);
            raw.back = Some(back);
            raw.class = parent_class.map(CardId::into_inner);
        }
        CardType::Statement(StatementCard { front }) => {
            raw.front = Some(front);
        }
        CardType::Event(EventCard {
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

pub fn new_raw_card(card: impl Into<CardType>) -> RawCard {
    let card: CardType = card.into();
    match card {
        CardType::Instance(concept) => new_concept(concept),
        CardType::Normal(normal) => new_normal(normal),
        CardType::Unfinished(unfinished) => new_unfinished(unfinished),
        CardType::Attribute(attribute) => new_attribute(attribute),
        CardType::Class(class) => new_class(class),
        CardType::Statement(statement) => new_statement(statement),
        CardType::Event(event) => new_event(event),
    }
}

pub fn new_unfinished(unfinished: UnfinishedCard) -> RawCard {
    RawCard {
        id: Uuid::new_v4(),
        data: from_any(unfinished.into()),
        ..Default::default()
    }
}

pub fn new_event(statement: EventCard) -> RawCard {
    RawCard {
        id: Uuid::new_v4(),
        data: from_any(statement.into()),
        ..Default::default()
    }
}

pub fn new_statement(statement: StatementCard) -> RawCard {
    RawCard {
        id: Uuid::new_v4(),
        data: from_any(statement.into()),
        ..Default::default()
    }
}

pub fn new_class(class: ClassCard) -> RawCard {
    RawCard {
        id: Uuid::new_v4(),
        data: from_any(class.into()),
        ..Default::default()
    }
}
pub fn new_attribute(attr: AttributeCard) -> RawCard {
    RawCard {
        id: Uuid::new_v4(),
        data: from_any(attr.into()),
        ..Default::default()
    }
}
pub fn new_concept(concept: InstanceCard) -> RawCard {
    RawCard {
        id: Uuid::new_v4(),
        data: from_any(concept.into()),
        ..Default::default()
    }
}
pub fn new_normal(normal: NormalCard) -> RawCard {
    RawCard {
        id: Uuid::new_v4(),
        data: from_any(normal.into()),
        ..Default::default()
    }
}

impl From<Card> for RawCard {
    fn from(card: Card) -> Self {
        RawCard {
            id: card.id.into_inner(),
            data: from_any(card.ty),
            dependencies: card
                .dependencies
                .into_iter()
                .map(|id| id.into_inner())
                .collect(),
            tags: card.tags,
            suspended: card.suspended.is_suspended(),
            deleted: false,
        }
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
        let value: Value = Deserialize::deserialize(deserializer)?;

        match value {
            Value::Boolean(b) => Ok(b.into()),
            Value::Integer(i) => {
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
