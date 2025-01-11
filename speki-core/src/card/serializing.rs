use omtrent::TimeStamp;

use super::{
    AttributeCard, CType, CardType, ClassCard, EventCard, InstanceCard, NormalCard, RawType,
    StatementCard, UnfinishedCard,
};

pub fn into_any(raw: RawType) -> CardType {
    match raw.ty {
        CType::Instance => InstanceCard {
            name: raw.front.unwrap(),
            class: raw.class.unwrap(),
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
            attribute: raw.attribute.unwrap(),
            back: raw.back.unwrap(),
            instance: raw.instance.unwrap(),
        }
        .into(),
        CType::Class => ClassCard {
            name: raw.front.unwrap(),
            back: raw.back.unwrap(),
            parent_class: raw.class,
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
            parent_event: raw.parent_event,
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
            raw.class = Some(class);
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
        }) => {
            raw.attribute = Some(attribute);
            raw.back = Some(back);
            raw.instance = Some(instance);
        }
        CardType::Class(ClassCard {
            name,
            back,
            parent_class,
        }) => {
            raw.front = Some(name);
            raw.back = Some(back);
            raw.class = parent_class;
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
            raw.parent_event = parent_event;
        }
    };

    raw
}
