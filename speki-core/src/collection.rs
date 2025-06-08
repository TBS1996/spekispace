use std::{
    cmp::Ordering,
    collections::{BTreeSet, HashSet},
    fmt::Display,
    sync::Arc,
};

use async_recursion::async_recursion;
use either::Either;
use ledgerstore::LedgerItem;
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{
    card::CardId,
    card_provider::CardProvider,
    ledger::{CollectionAction, CollectionEvent},
    Card,
};

pub type CollectionId = Uuid;

impl Display for Collection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.name)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Hash)]
pub struct Collection {
    pub id: CollectionId,
    pub name: String,
    pub dyncards: Vec<MaybeDyn>,
}

impl LedgerItem for Collection {
    type Error = ();
    type Key = CollectionId;
    type PropertyType = &'static str;
    type RefType = &'static str;
    type Modifier = CollectionAction;

    fn run_event(mut self, event: CollectionAction) -> Result<Self, Self::Error> {
        match event {
            CollectionAction::Delete => {}
            CollectionAction::SetName(s) => self.name = s,
            CollectionAction::InsertDyn(val) => self.dyncards.push(val),
            CollectionAction::RemoveDyn(val) => {
                self.dyncards.retain(|x| x != &val);
            }
        }

        Ok(self)
    }

    fn new_default(id: CollectionId) -> Self {
        Self {
            id,
            name: "uninit".to_string(),
            dyncards: Default::default(),
        }
    }

    fn item_id(&self) -> CollectionId {
        self.id
    }
}

impl Collection {
    pub fn new(name: String) -> Self {
        Self {
            id: CollectionId::new_v4(),
            name,
            dyncards: Default::default(),
        }
    }

    pub fn remove_dyn(&mut self, card: MaybeDyn) {
        self.dyncards.retain(|entry| entry != &card);
    }

    pub fn insert_dyn(&mut self, card: MaybeDyn) {
        if !self.dyncards.contains(&card) {
            self.dyncards.push(card);
        }
    }

    pub async fn expand_nodeps(&self, provider: CardProvider) -> BTreeSet<MaybeCard> {
        let mut cards = BTreeSet::<MaybeCard>::new();

        for card in &self.dyncards {
            cards.extend(card.evaluate(provider.clone()).await);
        }
        cards
    }

    #[async_recursion(?Send)]
    pub async fn expand(&self, provider: CardProvider) -> Vec<Arc<Card>> {
        let mut out: BTreeSet<Arc<Card>> = BTreeSet::default();
        let mut cards: Vec<MaybeCard> = Default::default();

        let mut futs = vec![];

        for card in &self.dyncards {
            let xprovider = provider.clone();
            futs.push(async move { card.evaluate(xprovider).await });
        }

        for x in futures::future::join_all(futs).await {
            cards.extend(x);
        }

        info!(
            "expanded cards wihtout deps for {}: {}",
            &self.name,
            cards.len()
        );

        let mut futs = vec![];

        for card in cards {
            let provider = provider.clone();
            let fut = async move {
                let mut out = vec![];

                match card {
                    MaybeCard::Id(id) => {
                        let Some(card) = provider.load(id) else {
                            warn!("unable to find card with id: {}", id);
                            return out;
                        };
                        for dep in card.recursive_dependencies() {
                            let dep = provider.load(dep).unwrap();
                            out.push(dep);
                        }
                        out.push(card);
                        out
                    }
                    MaybeCard::Card(card) => {
                        for dep in card.recursive_dependencies() {
                            let dep = provider.load(dep).unwrap();
                            out.push(dep);
                        }
                        out.push(card);
                        out
                    }
                }
            };

            futs.push(fut);
        }

        for cards in futures::future::join_all(futs).await {
            out.extend(cards);
        }

        out.into_iter().collect()
    }
}

#[derive(Eq, Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Hash)]
pub enum MaybeDyn {
    Collection(CollectionId),
    Dyn(DynCard),
}

impl MaybeDyn {
    #[async_recursion(?Send)]
    pub async fn expand(
        &self,
        provider: CardProvider,
        mut seen_cols: HashSet<CollectionId>,
    ) -> Vec<DynCard> {
        match self {
            MaybeDyn::Dyn(card) => vec![*card],
            MaybeDyn::Collection(id) => {
                seen_cols.insert(*id);
                let Some(col) = provider.providers.collections.load(*id) else {
                    return vec![];
                };

                let mut out = vec![];

                for maybe in col.dyncards {
                    out.extend(maybe.expand(provider.clone(), seen_cols.clone()).await);
                }

                out
            }
        }
    }

    pub async fn evaluate(&self, provider: CardProvider) -> Vec<MaybeCard> {
        let mut out = vec![];

        let cards = self.expand(provider.clone(), Default::default()).await;

        for card in cards {
            info!("dyn card to evaluate: {:?}", card);
            out.extend(card.evaluate(provider.clone()));
        }
        info!("done evaluating them cards");

        out
    }
}

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
    Any,
}

impl DynCard {
    fn card(&self) -> Option<CardId> {
        let card = match self {
            DynCard::Card(uuid) => uuid,
            DynCard::Instances(uuid) => uuid,
            DynCard::Dependents(uuid) => uuid,
            DynCard::RecDependents(uuid) => uuid,
            DynCard::Any => return None,
        };

        Some(*card)
    }
    pub fn display(&self, provider: CardProvider) -> String {
        let card = match self.card() {
            Some(id) => id,
            None => return "any".to_string(),
        };

        let name = provider.load(card).unwrap().name().to_string();

        match self {
            DynCard::Card(_) => name,
            DynCard::Instances(_) => format!("instances: {name}"),
            DynCard::Dependents(_) => format!("dependents: {name}"),
            DynCard::RecDependents(_) => format!("rec dependents: {name}"),
            DynCard::Any => unreachable!(),
        }
    }

    pub fn evaluate(&self, provider: CardProvider) -> Vec<MaybeCard> {
        match self {
            DynCard::Any => provider
                .load_all_card_ids()
                .into_iter()
                .map(MaybeCard::Id)
                .collect(),
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
