use std::{
    cmp::Ordering,
    collections::{BTreeSet, HashSet},
    fmt::Display,
    sync::Arc,
    time::Duration,
};

use async_recursion::async_recursion;
use serde::{Deserialize, Serialize};
use speki_dto::LedgerItem;
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

impl LedgerItem<CollectionEvent> for Collection {
    type Error = ();

    fn run_event(mut self, event: CollectionEvent) -> Result<Self, Self::Error> {
        match event.action {
            CollectionAction::SetName(s) => self.name = s,
            CollectionAction::InsertDyn(val) => self.dyncards.push(val),
            CollectionAction::RemoveDyn(val) => {
                self.dyncards.retain(|x| x != &val);
            }
        }

        Ok(self)
    }

    fn derive_events(&self) -> Vec<CollectionEvent> {
        todo!()
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
                        let Some(card) = provider.load(id).await else {
                            warn!("unable to find card with id: {}", id);
                            return out;
                        };
                        for dep in card.recursive_dependencies().await {
                            let dep = provider.load(dep).await.unwrap();
                            out.push(dep);
                        }
                        out.push(card);
                        return out;
                    }
                    MaybeCard::Card(card) => {
                        for dep in card.recursive_dependencies().await {
                            let dep = provider.load(dep).await.unwrap();
                            out.push(dep);
                        }
                        out.push(card);
                        return out;
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
                let Some(col) = provider.providers.collections.load(*id).await else {
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
        for card in self.expand(provider.clone(), Default::default()).await {
            info!("dyn card to evaluate: {:?}", card);
            out.extend(card.evaluate(provider.clone()).await);
        }
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

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq, Copy, Hash)]
pub enum DynCard {
    Card(CardId),
    Instances(CardId),
    Dependents(CardId),
    RecDependents(CardId),
    Any,
}

impl DynCard {
    pub async fn evaluate(&self, provider: CardProvider) -> Vec<MaybeCard> {
        match self {
            DynCard::Any => provider
                .load_all_card_ids()
                .await
                .into_iter()
                .map(MaybeCard::Id)
                .collect(),
            DynCard::Card(id) => vec![MaybeCard::Id(*id)],
            DynCard::Instances(id) => {
                let Some(card) = provider.load(*id).await else {
                    error!("failed to load card with id: {id}");
                    return vec![];
                };
                let mut output = vec![];

                for card in card.dependents().await {
                    if card.is_instance_of(*id) {
                        output.push(MaybeCard::Card(card));
                    }
                }

                output
            }
            DynCard::Dependents(id) => match provider.load(*id).await {
                Some(card) => card
                    .dependents()
                    .await
                    .into_iter()
                    .map(|x| MaybeCard::Card(x))
                    .collect(),
                None => vec![],
            },

            DynCard::RecDependents(id) => {
                let ids = match provider.load(*id).await {
                    Some(x) => x.recursive_dependents().await,
                    None => return vec![],
                };
                let mut out = vec![];

                for id in ids {
                    let card = provider.load(id).await.unwrap();
                    out.push(MaybeCard::Card(card));
                }

                out
            }
        }
    }
}
