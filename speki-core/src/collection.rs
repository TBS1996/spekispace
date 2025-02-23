use std::{
    cmp::Ordering, collections::{BTreeSet, HashSet}, fmt::Display, sync::Arc, time::Duration
};

use async_recursion::async_recursion;
use serde::{Deserialize, Serialize};
use speki_dto::{Item, ModifiedSource};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{card::CardId, card_provider::CardProvider, Card};

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
    pub last_modified: Duration,
    pub deleted: bool,
    pub source: ModifiedSource,
}

impl Collection {
    pub fn new(name: String) -> Self {
        Self {
            id: CollectionId::new_v4(),
            name,
            dyncards: Default::default(),
            last_modified: Default::default(),
            deleted: Default::default(),
            source: Default::default(),
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
            futs.push(async move {
                card.evaluate(xprovider).await
            });
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

        for cards in futures::future::join_all(futs).await{
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
            DynCard::Dependents(id) => 
            {

            match provider
                .load(*id)
                .await
                {
                    Some(card) => card.dependents().await.into_iter().map(|x|MaybeCard::Card(x)).collect(),
                    None =>  vec![],
                }

            }
            

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

impl Item for Collection {
    type PreviousVersion = prev::CollectionV1;
    type Key = Uuid;

    fn deleted(&self) -> bool {
        self.deleted
    }

    fn set_delete(&mut self) {
        self.deleted = true;
    }

    fn set_last_modified(&mut self, time: Duration) {
        self.last_modified = time;
    }

    fn last_modified(&self) -> Duration {
        self.last_modified
    }

    fn id(&self) -> Uuid {
        self.id
    }

    fn identifier() -> &'static str {
        "collections"
    }

    fn source(&self) -> ModifiedSource {
        self.source
    }

    fn set_source(&mut self, source: ModifiedSource) {
        self.source = source;
    }
}

mod prev {
    use tracing::info;

    use super::*;

    #[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq, Copy, Hash)]
    pub enum DynCard {
        Card(CardId),
        Instances(CardId),
        Dependents(CardId),
        RecDependents(CardId),
        Collection(CollectionId),
        Any,
    }

    impl From<DynCard> for super::MaybeDyn {
        fn from(value: DynCard) -> Self {
            match value {
                DynCard::Card(id) => MaybeDyn::Dyn(super::DynCard::Card(id)),
                DynCard::Instances(id) => MaybeDyn::Dyn(super::DynCard::Instances(id)),
                DynCard::Dependents(id) => MaybeDyn::Dyn(super::DynCard::Dependents(id)),
                DynCard::RecDependents(id) => MaybeDyn::Dyn(super::DynCard::RecDependents(id)),
                DynCard::Any => MaybeDyn::Dyn(super::DynCard::Any),
                DynCard::Collection(id) => MaybeDyn::Collection(id),
            }
        }
    }

    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Hash)]
    pub struct CollectionV1 {
        pub id: CollectionId,
        pub name: String,
        pub dyncards: Vec<DynCard>,
        pub last_modified: Duration,
        pub deleted: bool,
        pub source: ModifiedSource,
    }

    impl From<CollectionV1> for Collection {
        fn from(col: CollectionV1) -> Self {
            info!("converitng collectionv1 to col");
            Collection {
                id: col.id,
                name: col.name,
                dyncards: col
                    .dyncards
                    .into_iter()
                    .map(super::MaybeDyn::from)
                    .collect(),
                last_modified: col.last_modified,
                deleted: col.deleted,
                source: col.source,
            }
        }
    }

    impl Item for CollectionV1 {
        type PreviousVersion = Self;
        type Key = Uuid;

        fn deleted(&self) -> bool {
            self.deleted
        }

        fn set_delete(&mut self) {
            self.deleted = true;
        }

        fn set_last_modified(&mut self, time: Duration) {
            self.last_modified = time;
        }

        fn last_modified(&self) -> Duration {
            self.last_modified
        }

        fn id(&self) -> Uuid {
            self.id
        }

        fn identifier() -> &'static str {
            "collections"
        }

        fn source(&self) -> ModifiedSource {
            self.source
        }

        fn set_source(&mut self, source: ModifiedSource) {
            self.source = source;
        }
    }
}
