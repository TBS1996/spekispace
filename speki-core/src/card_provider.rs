use crate::{
    audio::Audio,
    card::CardId,
    ledger::{CardAction, CardEvent},
    metadata::Metadata,
    recall_rate::History,
    Card, Provider, Recaller, TimeGetter,
};
use dioxus_logger::tracing::{info, trace};
use snapstore::CacheKey;
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fmt::Debug,
    sync::{Arc, RwLock},
};

#[derive(Clone)]
pub struct Caches {
    inner: Arc<RwLock<HashMap<CacheKey, Arc<HashSet<String>>>>>,
    providers: Provider,
}

impl Debug for Caches {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Caches")
            .finish()
    }
}

impl Caches {
    pub fn new(providers: Provider) -> Self {
        Self {
            inner: Default::default(),
            providers,
        }
    }

    pub async fn get(&self, key: impl Into<CacheKey>) -> Arc<HashSet<String>> {
        let key: CacheKey = key.into();
        if let Some(set) = self.inner.read().unwrap().get(&key) {
            return set.clone();
        }

        let set: HashSet<String> = self.providers.cards.get_cache(key.clone()).await.into_iter().collect();

        let set = Arc::new(set);
        self.inner.write().unwrap().insert(key, set.clone());
        set
    }
}

#[derive(Clone)]
pub struct CardProvider {
    cards: Arc<RwLock<HashMap<CardId, Arc<Card>>>>,
    pub providers: Provider,
    time_provider: TimeGetter,
    recaller: Recaller,
    pub cache: Caches,
}

impl Debug for CardProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CardProvider")
            .field("inner", &":)")
            .finish()
    }
}

impl CardProvider {
    pub async fn remove_card(&self, card_id: CardId) {
        let event = CardEvent::new(card_id, CardAction::DeleteCard);
        self.providers.run_event(event).await;
    }

    pub async fn load_all_card_ids(&self) -> Vec<CardId> {
        info!("x1");
        self.providers
            .cards
            .load_ids()
            .await
            .into_iter()
            .map(|id| id.parse().unwrap())
            .collect()
    }

    pub async fn filtered_load<F, Fut>(&self, filter: F) -> Vec<Arc<Card>>
    where
        F: Fn(Arc<Card>) -> Fut + Send + Sync,
        Fut: std::future::Future<Output = bool>,
    {
        info!("loading card ids");
        let card_ids = self.load_all_card_ids().await;
        info!("so many ids loaded: {}", card_ids.len());

        let filtered_cards = futures::future::join_all(card_ids.into_iter().map(|id| {
            let filter = &filter;
            async move {
                match self.load(id).await {
                    Some(card) => {
                        if filter(card.clone()).await {
                            Some(card)
                        } else {
                            None
                        }
                    }
                    None => None,
                }
            }
        }))
        .await;

        filtered_cards.into_iter().filter_map(|card| card).collect()
    }

    pub async fn load_all(&self) -> Vec<Arc<Card>> {
        info!("load all");
        let filter = |_: Arc<Card>| async move { true };
        self.filtered_load(filter).await
    }

    pub async fn dependents(&self, id: CardId) -> BTreeSet<CardId> {
        trace!("dependents of: {}", id);
        //Arc::unwrap_or_clone(self.cache.get(CacheKey::Dependent(id)).await).into_iter().map(|x|x.parse().unwrap()).collect()
        todo!()
    }

    pub async fn load(&self, id: CardId) -> Option<Arc<Card>> {
        if let Some(card) = self.cards.read().unwrap().get(&id).cloned() {
            return Some(card);
        }

        let base = self.providers.cards.load(&id.to_string()).await?;
        let history = match self.providers.reviews.load(&id.to_string()).await {
            Some(revs) => revs,
            None => History::new(id),
        };
        let metadata = match self.providers.metadata.load(&id.to_string()).await {
            Some(meta) => meta,
            None => Metadata::new(id),
        };

        let front_audio: Option<Audio> = None;
        let back_audio: Option<Audio> = None;

        let card = Arc::new(
            Card::from_parts(
                base,
                history,
                metadata,
                self.clone(),
                self.recaller.clone(),
                front_audio,
                back_audio,
            )
            .await,
        );

        self.cards.write().unwrap().insert(id, card.clone());

        Some(card)
    }

    pub fn invalidate_card(&self, id: CardId) -> Option<Arc<Card>> {
        self.cards.write().unwrap().remove(&id)
    }

    pub fn time_provider(&self) -> TimeGetter {
        self.time_provider.clone()
    }

    pub fn new(provider: Provider, time_provider: TimeGetter, recaller: Recaller) -> Self {
        Self {
            cards: Default::default(),
            time_provider,
            recaller,
            cache: Caches::new(provider.clone()),
            providers: provider,
        }
    }
}
