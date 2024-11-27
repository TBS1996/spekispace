use crate::{
    card::serializing::{into_any, into_raw_card},
    reviews::Reviews,
    AnyType, Attribute, Card, Provider, Recaller, TimeGetter,
};
use dioxus_logger::tracing::{info, trace};
use speki_dto::{AttributeId, CardId, Review};
use std::{
    collections::HashMap,
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
}

impl Debug for CardProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CardProvider")
            .field("inner", &":)")
            .finish()
    }
}

impl CardProvider {
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

    pub async fn fill_cache(&self) {
        self.load_all().await;
    }

    pub fn new(provider: Provider, time_provider: TimeGetter, recaller: Recaller) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner {
                cards: Default::default(),
                reviews: Default::default(),
            })),
            time_provider,
            provider,
            recaller,
        }
    }

    async fn load_uncached(&self, id: CardId) -> Option<Card<AnyType>> {
        let raw_card = self.provider.load_card(id).await?;
        let reviews = self.provider.load_reviews(id).await;
        let history = Reviews(reviews);
        let data = into_any(raw_card.data, self);

        let card = Card::<AnyType> {
            id,
            data,
            dependencies: raw_card.dependencies.into_iter().map(CardId).collect(),
            tags: raw_card.tags,
            history,
            suspended: crate::card::IsSuspended::from(raw_card.suspended),
            card_provider: self.clone(),
            recaller: self.recaller.clone(),
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
        let last_modified = self.provider.last_modified_card(id).await;
        if last_modified > cached.fetched {
            trace!("review cache outdated for card: {}", id);
            None
        } else {
            trace!("successfully retrieved review cache for card: {}", id);
            Some(cached.review.clone())
        }
    }

    async fn load_cached_card(&self, id: CardId) -> Option<Arc<Card<AnyType>>> {
        trace!("attempting cache load for card: {}", id);
        let guard = self.inner.read().unwrap();
        let cached = match guard.cards.get(&id) {
            Some(cached) => cached,
            None => {
                trace!("cache miss for card: {}", id);
                return None;
            }
        };

        let last_modified = self.provider.last_modified_card(id).await;
        if last_modified > cached.fetched {
            trace!("cache outdated for card: {}", id);
            None
        } else {
            trace!("successfully retrieved cache for card: {}", id);
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
        let cached_card = CardCache { fetched: now, card };

        guard.cards.insert(id, cached_card);
        guard.reviews.insert(id, cached_reviews);
    }

    pub async fn load_all_card_ids(&self) -> Vec<CardId> {
        self.provider.load_card_ids().await
    }

    pub async fn load_all(&self) -> Vec<Arc<Card<AnyType>>> {
        info!("loading card ids");
        let card_ids = self.load_all_card_ids().await;
        info!("so many ids loaded: {}", card_ids.len());

        let output = futures::future::join_all(card_ids.into_iter().map(|id| async move {
            info!("loading card..");
            let card = self.load(id).await.unwrap();
            info!("loaded card");
            card
        }))
        .await;

        output
    }

    pub async fn load(&self, id: CardId) -> Option<Arc<Card<AnyType>>> {
        dbg!();
        trace!("loading card for id: {}", id);
        if let (Some(card), Some(_)) = (
            self.load_cached_card(id).await,
            self.load_cached_reviews(id).await,
        ) {
            Some(card)
        } else {
            dbg!();
            trace!("cache miss for id: {}", id);
            dbg!();
            let uncached = self.load_uncached(id).await?;
            dbg!();
            let uncached = Arc::new(uncached);
            self.update_cache(uncached.clone());
            Some(uncached)
        }
    }

    pub async fn load_attribute(&self, id: AttributeId) -> Option<Attribute> {
        self.provider
            .load_attribute(id)
            .await
            .map(|dto| Attribute::from_dto(dto, self.clone()))
    }
}

struct Inner {
    cards: HashMap<CardId, CardCache>,
    reviews: HashMap<CardId, RevCache>,
}

struct CardCache {
    fetched: Duration,
    card: Arc<Card<AnyType>>,
}

struct RevCache {
    fetched: Duration,
    review: Reviews,
}
