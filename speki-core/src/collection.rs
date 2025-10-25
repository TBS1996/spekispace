use serde::{Deserialize, Serialize};

use crate::{
    card::{CType, CardId},
    card_provider::CardProvider,
};

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq, Copy, Hash, PartialOrd, Ord)]
pub enum DynCard {
    /// Get all instances of a class.
    Instances(CardId),
    /// Get all direct dependents of a card.
    Dependents(CardId),
    /// Get all recursive dependents of a card.
    RecDependents(CardId),
    /// Get all cards of a specific type.
    CardType(CType),
    /// Get all trivial cards.
    Trivial(bool),
}

impl DynCard {
    pub fn display(&self, provider: CardProvider) -> String {
        let name = |id: &CardId| {
            provider
                .load(*id)
                .map(|card| card.name().to_string())
                .unwrap_or("<invalid card>".to_string())
        };

        match self {
            DynCard::Trivial(flag) => format!("trivial: {}", flag),
            DynCard::Instances(id) => format!("instances: {}", name(id)),
            DynCard::Dependents(id) => format!("dependents: {}", name(id)),
            DynCard::RecDependents(id) => format!("dependents: {}", name(id)),
            DynCard::CardType(ctype) => {
                format!("card type: {ctype}")
            }
        }
    }
}
