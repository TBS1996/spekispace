use crate::{
    card::{
        serializing::{into_any, into_raw_card},
        RecallRate,
    },
    reviews::Reviews,
    AnyType, Attribute, Card, Provider, Recaller, TimeGetter,
};
use dioxus_logger::tracing::{info, trace};
use speki_dto::{AttributeId, CardId, Review};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fmt::Debug,
    sync::{Arc, RwLock},
    time::Duration,
};

#[derive(Clone)]
pub struct CardProvider {
    inner: Arc<RwLock<Inner>>,
    provider: Provider,
    time_provider: TimeGetter,
    recaller: Recaller,
    check_modified: bool,
}

impl Debug for CardProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CardProvider")
            .field("inner", &":)")
            .finish()
    }
}

use std::future::Future;
use std::pin::Pin;

impl CardProvider {
    pub fn min_rec_recall_rate(
        &self,
        id: CardId,
    ) -> Pin<Box<dyn Future<Output = RecallRate> + '_>> {
        Box::pin(async move {
            trace!("card: {id} starting min rec recall calculation");
            let card = self.load(id).await.unwrap();
            let entry = self.load_cached_entry(id).await.unwrap();
            let recall_rate = card.recall_rate().unwrap_or_default();

            if let Some(recall) = entry.min_rec_recall {
                trace!("card: {id}: cached min rec recall: {recall}");
                return recall;
            }

            let dependencies = card.dependency_ids().await;

            if dependencies.is_empty() {
                trace!("card: {id}: no dependencies!");
                self.update_min_rec_recall(id, 1.0);
                recall_rate
            } else {
                trace!("card: {id} traversing dependencies first: {dependencies:?}");
                let mut min_recall: RecallRate = 1.0;

                for dep in dependencies {
                    let rec = self.min_rec_recall_rate(dep).await;
                    let dep_rec = self
                        .load(dep)
                        .await
                        .unwrap()
                        .recall_rate()
                        .unwrap_or_default();
                    min_recall = min_recall.min(rec);
                    min_recall = min_recall.min(dep_rec);
                    trace!("card: {id}: min recall updated: {min_recall}");
                }

                self.update_min_rec_recall(id, min_recall);
                trace!("card: {id}: new min rec recall: {min_recall}");
                min_recall
            }
        })
    }

    fn update_min_rec_recall(&self, id: CardId, cached_recall: RecallRate) {
        trace!("card: {id}: caching min rec recall: {cached_recall}");
        let mut guard = self.inner.write().unwrap();
        guard.cards.get_mut(&id).unwrap().min_rec_recall = Some(cached_recall);
    }

    pub async fn load_all_card_ids(&self) -> Vec<CardId> {
        self.provider.load_card_ids().await
    }

    pub async fn fill_cache(&self) {
        let mut cards: HashMap<CardId, CardCache> = Default::default();
        let mut rev_caches: HashMap<CardId, RevCache> = Default::default();
        let raw_cards = self.provider.load_all_cards().await;
        let mut reviews = self.provider.load_all_reviews().await;
        let fetched = self.time_provider.current_time();

        for card in raw_cards {
            let rev = reviews.remove(&card.id).unwrap_or_default();
            let card =
                Card::from_raw_with_reviews(card, self.clone(), self.recaller.clone(), rev.clone());
            let card = Arc::new(card);
            self.update_dependents(card.clone()).await;

            let reventry = RevCache {
                fetched,
                review: Reviews(rev),
            };

            let entry = CardCache {
                fetched,
                card,
                min_rec_recall: None,
            };

            rev_caches.insert(entry.card.id, reventry);
            cards.insert(entry.card.id, entry);
        }

        let mut guard = self.inner.write().unwrap();
        guard.cards = cards;
        guard.reviews = rev_caches;
    }

    pub async fn filtered_load<F, Fut>(&self, filter: F) -> Vec<Arc<Card<AnyType>>>
    where
        F: Fn(Arc<Card<AnyType>>) -> Fut + Send + Sync,
        Fut: std::future::Future<Output = bool>,
    {
        info!("loading card ids");
        let card_ids = self.load_all_card_ids().await;
        info!("so many ids loaded: {}", card_ids.len());

        let filtered_cards = futures::future::join_all(card_ids.into_iter().map(|id| {
            let filter = &filter;
            async move {
                let card = self.load(id).await.unwrap();

                if filter(card.clone()).await {
                    Some(card)
                } else {
                    None
                }
            }
        }))
        .await;

        filtered_cards.into_iter().filter_map(|card| card).collect()
    }

    pub async fn load_all(&self) -> Vec<Arc<Card<AnyType>>> {
        let filter = |_: Arc<Card<AnyType>>| async move { true };
        self.filtered_load(filter).await
    }

    pub async fn dependents(&self, id: CardId) -> BTreeSet<Arc<Card<AnyType>>> {
        info!("dependents of: {}", id);
        let mut out = BTreeSet::default();
        let deps = self
            .inner
            .read()
            .unwrap()
            .dependents
            .get(&id)
            .cloned()
            .unwrap_or_default();

        for dep in deps {
            if let Some(card) = self.load(dep).await {
                out.insert(card);
            }
        }

        out
    }

    pub async fn load(&self, id: CardId) -> Option<Arc<Card<AnyType>>> {
        trace!("loading card for id: {}", id);
        if let (Some(card), Some(_)) = (
            self.load_cached_card(id).await,
            self.load_cached_reviews(id).await,
        ) {
            trace!("cache hit for id: {}", id);
            Some(card)
        } else {
            trace!("cache miss for id: {}", id);
            self.fresh_load(id).await
        }
    }

    pub async fn load_attribute(&self, id: AttributeId) -> Option<Attribute> {
        let modified = self.provider.last_modified_attribute(id).await;
        self.provider
            .load_attribute(id)
            .await
            .map(|dto| Attribute::from_dto(dto, self.clone(), modified))
    }
    pub async fn load_reviews(&self, id: CardId) -> Reviews {
        Reviews(self.provider.load_reviews(id).await)
    }

    pub async fn save_reviews(&self, id: CardId, reviews: Reviews) {
        self.provider.save_reviews(id, reviews.into_inner()).await;
    }

    pub async fn add_review(&self, id: CardId, review: Review) {
        self.provider.add_review(id, review).await;
    }

    pub async fn delete_card(&self, id: CardId) {
        self.provider.delete_card(id).await;
    }

    pub async fn save_card(&self, card: Card<AnyType>) {
        self.update_cache(Arc::new(card.clone()));
        let raw = into_raw_card(card);
        self.provider.save_card(raw).await;
    }

    pub fn time_provider(&self) -> TimeGetter {
        self.time_provider.clone()
    }

    pub fn new(provider: Provider, time_provider: TimeGetter, recaller: Recaller) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner {
                cards: Default::default(),
                reviews: Default::default(),
                dependents: Default::default(),
            })),
            time_provider,
            provider,
            recaller,
            check_modified: false,
        }
    }

    async fn load_uncached(&self, id: CardId) -> Option<Card<AnyType>> {
        trace!("load uncached");
        let raw_card = self.provider.load_card(id).await?;
        let reviews = self.provider.load_reviews(id).await;
        let history = Reviews(reviews);
        let data = into_any(raw_card.data, self);
        let last_modified = self.provider.last_modified_card(id).await;

        let card = Card::<AnyType> {
            id,
            data,
            dependencies: raw_card.dependencies.into_iter().map(CardId).collect(),
            tags: raw_card.tags,
            history,
            suspended: crate::card::IsSuspended::from(raw_card.suspended),
            card_provider: self.clone(),
            recaller: self.recaller.clone(),
            last_modified,
        };

        Some(card)
    }

    async fn load_cached_reviews(&self, id: CardId) -> Option<Reviews> {
        trace!("attempting review cache load for: {}", id);
        let guard = self.inner.read().unwrap();
        let cached = match guard.reviews.get(&id) {
            Some(cached) => cached,
            None => {
                trace!("cache miss for review: {}", id);
                return None;
            }
        };

        if self.check_modified {
            let last_modified = self.provider.last_modified_card(id).await;
            if last_modified > cached.fetched {
                trace!("review cache outdated for card: {}", id);
                None
            } else {
                trace!("successfully retrieved review cache for card: {}", id);
                Some(cached.review.clone())
            }
        } else {
            Some(cached.review.clone())
        }
    }

    async fn update_dependents(&self, card: Arc<Card<AnyType>>) {
        trace!("updating cache dependents");
        let mut guard = self.inner.write().unwrap();
        for dep in card.dependency_ids().await {
            guard.dependents.entry(dep).or_default().insert(card.id);
        }
    }

    async fn load_cached_entry(&self, id: CardId) -> Option<CardCache> {
        trace!("attempting cache load for card: {}", id);
        let guard = self.inner.read().unwrap();
        trace!("cache size: {}", guard.cards.len());
        match guard.cards.get(&id) {
            Some(cached) => Some((*cached).clone()),
            None => {
                trace!("cache miss for card: {}", id);
                None
            }
        }
    }

    async fn load_cached_card(&self, id: CardId) -> Option<Arc<Card<AnyType>>> {
        let cached = self.load_cached_entry(id).await?;

        if self.check_modified {
            let last_modified = self.provider.last_modified_card(id).await;
            if last_modified > cached.fetched {
                info!("cache outdated for card: {}", id);
                None
            } else {
                info!("successfully retrieved cache for card: {}", id);
                Some(cached.card.clone())
            }
        } else {
            Some(cached.card.clone())
        }
    }

    fn update_cache(&self, card: Arc<Card<AnyType>>) {
        trace!("updating cache for card: {}", card.id);
        let now = self.time_provider.current_time();
        let mut guard = self.inner.write().unwrap();
        let id = card.id;

        let cached_reviews = RevCache {
            fetched: now,
            review: card.history.clone(),
        };
        let cached_card = CardCache {
            fetched: now,
            card,
            min_rec_recall: None,
        };

        guard.cards.insert(id, cached_card);
        guard.reviews.insert(id, cached_reviews);
    }

    async fn fresh_load(&self, id: CardId) -> Option<Arc<Card<AnyType>>> {
        let uncached = self.load_uncached(id).await?;
        let uncached = Arc::new(uncached);
        self.update_dependents(uncached.clone()).await;
        self.update_cache(uncached.clone());
        Some(uncached)
    }
}

struct Inner {
    cards: HashMap<CardId, CardCache>,
    reviews: HashMap<CardId, RevCache>,
    dependents: HashMap<CardId, HashSet<CardId>>,
}

#[derive(Clone, Debug)]
struct CardCache {
    fetched: Duration,
    card: Arc<Card<AnyType>>,
    min_rec_recall: Option<RecallRate>,
}

struct RevCache {
    fetched: Duration,
    review: Reviews,
}
