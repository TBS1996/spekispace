use std::collections::{BTreeMap, BTreeSet};

use crate::{
    audio::AudioId,
    card::{AttributeId, Attrv2, BackSide, CardId, ParamAnswer, RawCard, TextData},
    metadata::Metadata,
    recall_rate::{Review, ReviewEvent},
};
use ledgerstore::LedgerEvent;
use omtrent::TimeStamp;
use serde::{Deserialize, Serialize};

pub type CardEvent = LedgerEvent<RawCard>;

#[derive(Deserialize, Serialize, Clone, Debug, Hash)]
pub enum CardAction {
    /// Sets front audio. To remove, Set it to None.
    SetFrontAudio(Option<AudioId>),
    /// Sets back audio. To remove, Set it to None.
    SetBackAudio(Option<AudioId>),
    /// Removes explicit dependency. Dependency must exist.
    RemoveDependency(CardId),
    /// Insert explicit dependency.
    AddDependency(CardId),
    /// Sets answer side to a given card.
    SetBackRef(CardId),
    /// Sets answer to be of type boolean.
    SetBackText(TextData),
    /// Sets answer to be of type boolean.
    SetBackBool(bool),
    /// Set answer to card to a given timestamp.
    SetBackTime(TimeStamp),
    /// Sets or unsets a given namespace for a card.
    ///
    /// Namespaces are used for when a card only makes sense in a given context.
    SetNamespace(Option<CardId>),

    /// Inserts an attribute to a class card.
    InsertAttr(Attrv2),
    /// Inserts multiple attributes to a class card. Will override existing attributes.
    SetAttrs(BTreeSet<Attrv2>),
    /// Removes an attribute from a class card.
    RemoveAttr(AttributeId),

    /// Sets parameters to a class card. Will override existing parameters.
    ///
    /// A parameter is a something that helps identify a specific instance. It's related to attributes but with a key distinction.
    /// While an attribute is an additional question about a given instance, a parameter is something to identify the instance itself.
    /// For example, "date of birth" can be an attribute on "Person" class, you don't need to know the date of birth of a person to "know" about this person.
    /// So it's something that is asked after you already know about the person, its a separate card that has the person as an implicit dependency.
    /// But for example, if you have a class `Rust module`, you need to know in which rust crate this module is defined in. So this would be a parameter.
    /// As the crate a module is defined by is an essential part of this module's identity.
    /// We do however use the same underlying struct `Attrv2` just from coincidentally they need the same data.
    SetParams(BTreeSet<Attrv2>),
    /// Removes a param from a class card.
    RemoveParam(AttributeId),
    /// Insert a param to a class card.
    InsertParam(Attrv2),

    /// Sets parameter values to an instance card. Will override existing param values.
    ///
    /// The class of this instance must have a parameter with the given attributeId, and the answer must be of correct type if the parameter has a backtype constraint.
    SetParamAnswers(BTreeMap<AttributeId, ParamAnswer>),

    /// Inserts an answer to a parameter.
    InsertParamAnswer {
        id: AttributeId,
        answer: ParamAnswer,
    },
    /// Removes a parameter value from an instance card.
    RemoveParamAnswer(AttributeId),

    /// Sets (or unsets) the parent class of a given class.
    ///
    /// Any class may have a parent class from which it will inherit attributes and parameters.
    /// And also, the instances of a class are also considered to be instances of the parent class (recursively).
    SetParentClass(Option<CardId>),
    /// Sets class of a given instance.
    ///
    /// Non-optional as every instance must have a class.
    SetInstanceClass(CardId),

    /// Replaces all references to `current` with `other` in this card.
    /// Useful when merging duplicate cards.
    ReplaceDependency { current: CardId, other: CardId },

    /// Creates an attribute type card.
    ///
    /// Attribute cards are the answers to an attribute of an instance card.
    AttributeType {
        attribute: AttributeId,
        back: BackSide,
        instance: CardId,
    },
    /// Creates a normal card.
    NormalType { front: TextData, back: BackSide },
    /// Creates an instance card.
    InstanceType { front: TextData, class: CardId },
    /// Creates a statement card.
    ///
    /// Statement cards are for cards that cannot easily be turned into a question, but still nice to have in order
    /// to create a proper knowledge graph.
    StatementType { front: TextData },
    /// Creates a class card.
    ClassType { front: TextData },

    /// Sets the backside of a card.
    SetBackside(Option<BackSide>),
    /// sets the front side of a card.
    SetFront(TextData),

    /// Creates an unfinished card.
    UnfinishedType { front: TextData },
}

pub enum HistoryEvent {
    Review { id: CardId, review: Review },
}

pub type MetaEvent = LedgerEvent<Metadata>;

#[derive(Debug, Clone, Serialize, Deserialize, Hash)]
pub enum MetaAction {
    Suspend(bool),
    SetTrivial(Option<bool>),
    SetNeedsWork(bool),
}

impl From<MetaEvent> for Event {
    fn from(event: MetaEvent) -> Self {
        Event::Meta(event)
    }
}
impl From<CardEvent> for Event {
    fn from(event: CardEvent) -> Self {
        Event::Card(event)
    }
}
impl From<ReviewEvent> for Event {
    fn from(event: ReviewEvent) -> Self {
        Event::History(event)
    }
}

pub enum Event {
    Meta(MetaEvent),
    History(ReviewEvent),
    Card(CardEvent),
}
