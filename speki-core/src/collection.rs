use std::{cmp::Ordering, sync::Arc};

use ledgerstore::{PropertyCache, RefGetter, TheCacheGetter};
use serde::{Deserialize, Serialize};

use crate::{
    card::{CType, CardId},
    card_provider::CardProvider,
    Card, CardProperty, CardRefType,
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
    Instances(CardId),
    Dependents(CardId),
    RecDependents(CardId),
    CardType(CType),
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

    pub fn evaluate(&self, provider: CardProvider) -> Vec<MaybeCard> {
        match self {
            DynCard::Instances(id) => {
                let mut output = vec![];
                let getter = ledgerstore::TheCacheGetter::ItemRef(RefGetter {
                    reversed: true,
                    key: *id,
                    ty: Some(CardRefType::ParentClass),
                    recursive: true,
                });
                let mut all_classes = dbg!(provider.providers.cards.load_getter(getter));
                all_classes.insert(*id);

                for class in all_classes {
                    let getter = TheCacheGetter::ItemRef(RefGetter {
                        reversed: true,
                        key: class,
                        ty: Some(CardRefType::ClassOfInstance),
                        recursive: false,
                    });
                    for instance in provider.providers.cards.load_getter(getter) {
                        output.push(MaybeCard::Id(instance));
                    }
                }

                output
            }
            DynCard::Trivial(flag) => provider
                .providers
                .cards
                .get_prop_cache(PropertyCache::new(CardProperty::Trivial, flag.to_string()))
                .into_iter()
                .map(|id| MaybeCard::Id(id))
                .collect(),
            DynCard::CardType(ty) => provider
                .providers
                .cards
                .get_prop_cache(PropertyCache::new(CardProperty::CardType, ty.to_string()))
                .into_iter()
                .map(|id| MaybeCard::Id(id))
                .collect(),

            DynCard::Dependents(id) => match provider.load(*id) {
                Some(card) => card.dependents().into_iter().map(MaybeCard::Card).collect(),
                None => vec![],
            },

            DynCard::RecDependents(id) => {
                dbg!("rec dependents");
                let ids = match dbg!(provider.load(*id)) {
                    Some(x) => x.recursive_dependents(),
                    None => return vec![],
                };

                let mut out = vec![];

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
