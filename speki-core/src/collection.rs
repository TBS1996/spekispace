use std::{cmp::Ordering, sync::Arc};

use serde::{Deserialize, Serialize};
use tracing::error;

use crate::{card::CardId, card_provider::CardProvider, Card};

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum MaybeCard {
    Id(CardId),
    Card(Arc<Card>),
}

impl MaybeCard {
    pub fn id(&self) -> CardId {
        match self {
            Self::Id(id) => *id,
            Self::Card(ref card) => card.id(),
        }
    }
}

impl Ord for MaybeCard {
    fn cmp(&self, other: &Self) -> Ordering {
        self.id().cmp(&other.id())
    }
}

impl PartialOrd for MaybeCard {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq, Copy, Hash, PartialOrd, Ord)]
pub enum DynCard {
    Card(CardId),
    Instances(CardId),
    Dependents(CardId),
    RecDependents(CardId),
}

impl DynCard {
    fn card(&self) -> CardId {
        let card = match self {
            DynCard::Card(uuid) => uuid,
            DynCard::Instances(uuid) => uuid,
            DynCard::Dependents(uuid) => uuid,
            DynCard::RecDependents(uuid) => uuid,
        };

        *card
    }
    pub fn display(&self, provider: CardProvider) -> String {
        let card = self.card();

        let name = provider.load(card).unwrap().name().to_string();

        match self {
            DynCard::Card(_) => name,
            DynCard::Instances(_) => format!("instances: {name}"),
            DynCard::Dependents(_) => format!("dependents: {name}"),
            DynCard::RecDependents(_) => format!("rec dependents: {name}"),
        }
    }

    pub fn evaluate(&self, provider: CardProvider) -> Vec<MaybeCard> {
        match self {
            DynCard::Card(id) => vec![MaybeCard::Id(*id)],
            DynCard::Instances(id) => {
                let Some(card) = provider.load(*id) else {
                    error!("failed to load card with id: {id}");
                    return vec![];
                };
                let mut output = vec![];

                for card in card.dependents() {
                    if card.is_instance_of(*id) {
                        output.push(MaybeCard::Card(card));
                    }
                }

                output
            }
            DynCard::Dependents(id) => match provider.load(*id) {
                Some(card) => card.dependents().into_iter().map(MaybeCard::Card).collect(),
                None => vec![],
            },

            DynCard::RecDependents(id) => {
                let ids = match provider.load(*id) {
                    Some(x) => x.recursive_dependents(),
                    None => return vec![],
                };
                let mut out = vec![];

                for id in ids {
                    let card = provider.load(id).unwrap();
                    out.push(MaybeCard::Card(card));
                }

                out
            }
        }
    }
}
