use std::{cmp::Ordering, sync::Arc};

use serde::{Deserialize, Serialize};

use crate::{
    card::{CType, CardId},
    card_provider::CardProvider,
    Card, CardProperty, RefType,
};

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
    CardType(CType),
}

impl DynCard {
    pub fn display(&self, provider: CardProvider) -> String {
        let name = |id: &CardId| provider.load(*id).unwrap().name().to_string();

        match self {
            DynCard::Card(id) => name(id),
            DynCard::Instances(id) => format!("instances: {}", name(id)),
            DynCard::Dependents(id) => format!("dependents: {}", name(id)),
            DynCard::RecDependents(id) => format!("rec dependents: {}", name(id)),
            DynCard::CardType(ctype) => {
                format!("card type: {ctype}")
            }
        }
    }

    pub fn evaluate(&self, provider: CardProvider) -> Vec<MaybeCard> {
        match self {
            DynCard::Card(id) => vec![MaybeCard::Id(*id)],
            DynCard::Instances(id) => {
                let mut output = vec![];

                for instance in provider
                    .providers
                    .cards
                    .get_ref_cache(RefType::Instance, *id)
                {
                    output.push(MaybeCard::Id(instance.parse().unwrap()));
                }

                output
            }
            DynCard::CardType(ty) => provider
                .providers
                .cards
                .get_prop_cache(CardProperty::CardType, ty.to_string())
                .into_iter()
                .map(|id| MaybeCard::Id(id.parse().unwrap()))
                .collect(),

            DynCard::Dependents(id) => match provider.load(*id) {
                Some(card) => card.dependents().into_iter().map(MaybeCard::Card).collect(),
                None => vec![],
            },

            DynCard::RecDependents(id) => {
                dbg!("rec dependents");
                let ids = match provider.load(*id) {
                    Some(x) => x.recursive_dependents(),
                    None => return vec![],
                };
                let mut out = vec![];

                dbg!(ids.len());
                for (idx, id) in ids.into_iter().enumerate() {
                    if idx % 50 == 0 {
                        dbg!(idx);
                    }

                    out.push(MaybeCard::Id(id));
                }
                dbg!();

                out
            }
        }
    }
}
