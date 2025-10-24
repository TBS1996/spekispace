use ledgerstore::{ItemExpr, ItemNode, Leaf, PropertyCache, RefGetter};
use serde::{Deserialize, Serialize};

use crate::{
    card::{CType, CardId, RawCard},
    card_provider::CardProvider,
    CardProperty, CardRefType,
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

    pub fn to_node(&self) -> ItemNode<RawCard> {
        match self {
            DynCard::Instances(id) => {
                let leaf = Leaf::Reference(RefGetter {
                    reversed: true,
                    key: *id,
                    ty: Some(CardRefType::ParentClass),
                    recursive: true,
                });

                ItemNode::Leaf(leaf)
            }
            DynCard::Dependents(id) => {
                let leaf = Leaf::Reference(RefGetter {
                    reversed: true,
                    key: *id,
                    ty: None,
                    recursive: false,
                });

                ItemNode::Leaf(leaf)
            }
            DynCard::RecDependents(id) => {
                let leaf = Leaf::Reference(RefGetter {
                    reversed: true,
                    key: *id,
                    ty: None,
                    recursive: true,
                });

                ItemNode::Leaf(leaf)
            }
            DynCard::CardType(ty) => {
                let leaf = Leaf::Property(PropertyCache {
                    property: CardProperty::CardType,
                    value: ty.to_string(),
                });

                ItemNode::Leaf(leaf)
            }

            DynCard::Trivial(flag) => {
                let leaf = Leaf::Property(PropertyCache {
                    property: CardProperty::Trivial,
                    value: flag.to_string(),
                });

                ItemNode::Leaf(leaf)
            }
        }
    }

    pub fn evaluate(&self, provider: CardProvider) -> Vec<CardId> {
        match self {
            DynCard::Instances(id) => {
                let sub_classes: ItemExpr<RawCard> = ItemExpr::Reference {
                    items: Box::new(ItemExpr::Item(*id)),
                    ty: Some(CardRefType::ParentClass),
                    reversed: true,
                    recursive: true,
                    include_self: true,
                };

                let expr = ItemExpr::Reference {
                    items: Box::new(sub_classes),
                    ty: Some(CardRefType::ClassOfInstance),
                    reversed: true,
                    recursive: true,
                    include_self: false,
                };

                provider
                    .providers
                    .cards
                    .load_expr(expr)
                    .into_iter()
                    .collect()
            }
            DynCard::Trivial(flag) => provider
                .providers
                .cards
                .get_prop_cache(PropertyCache::new(CardProperty::Trivial, flag.to_string()))
                .into_iter()
                .collect(),
            DynCard::CardType(ty) => provider
                .providers
                .cards
                .get_prop_cache(PropertyCache::new(CardProperty::CardType, ty.to_string()))
                .into_iter()
                .collect(),

            DynCard::Dependents(id) => match provider.load(*id) {
                Some(card) => card.recursive_dependent_ids().into_iter().collect(),
                None => Default::default(),
            },

            DynCard::RecDependents(id) => {
                dbg!("rec dependents");
                match dbg!(provider.load(*id)) {
                    Some(x) => x.recursive_dependents().into_iter().collect(),
                    None => vec![],
                }
            }
        }
    }
}
