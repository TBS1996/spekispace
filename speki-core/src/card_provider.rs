use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fmt::Debug,
    future::Future,
    pin::Pin,
    sync::{Arc, RwLock},
    time::Duration,
};

use dioxus_logger::tracing::{info, trace};
use eyre::Result;
use speki_dto::Item;
use uuid::Uuid;

use crate::{
    card::{BaseCard, CardId, RecallRate},
    metadata::Metadata,
    recall_rate::History,
    Card, Provider, Recaller, TimeGetter,
};

/// Card cache
///
/// Has basically two functions. One is caching stuff in-memory so it'll be faster to load cards.
/// The other is to
#[derive(Clone)]
pub struct CardProvider {
    inner: Arc<RwLock<Inner>>,
    pub provider: Provider,
    time_provider: TimeGetter,
    recaller: Recaller,
    check_modified: bool,
    indices: Arc<RwLock<BTreeMap<String, BTreeSet<Uuid>>>>,
}

impl Debug for CardProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CardProvider")
            .field("inner", &":)")
            .finish()
    }
}

impl CardProvider {
    /// Removes a card from the cache, along with all its dependents and dependencies
    ///
    /// This is because certain properties are cached based on dependencies, so when a card in cache is no longer valid,
    /// the dependencies/dependents are also no longer guaranteed to be valid.
    /// For example, when you make a review, it'll change the recall rate of the given card, but it'll also change the min_rec_recall_rate of
    /// all its dependents, which we store in the cache.
    pub async fn invalidate_card_and_deps(&self, id: CardId) {
        info!("invalidating card with deps: {id}");
        let card = self.load(id).await.unwrap();

        self.invalidate_card(id).await;

        for dep in card.all_dependents().await {
            self.invalidate_card(dep).await;
        }

        for dep in card.all_dependencies().await {
            self.invalidate_card(dep).await;
        }

        info!("done invalidating card with deps");
    }

    pub async fn get_indices(&self, word: String) -> BTreeSet<Uuid> {
        if let Some(set) = self.indices.read().unwrap().get(&word) {
            return set.clone();
        }

        let set = self.provider.cards.load_indices(word.clone()).await;
        self.indices.write().unwrap().insert(word, set.clone());
        set
    }

    pub async fn invalidate_card(&self, id: CardId) {
        info!("invalidating card: {id}");
        let mut guard = self.inner.write().unwrap();

        let Some(card) = guard.cards.remove(&id) else {
            info!("oops no card");
            return;
        };

        guard.metadata.remove(&id);
        guard.reviews.remove(&id);

        drop(guard);

        for dependency in card.card.dependency_ids().await {
            let dependent = card.card.id();
            self.rm_dependent(dependency, dependent);
        }
        info!("done invalidating");
    }

    pub fn rm_dependent(&self, dependency: CardId, dependent: CardId) -> bool {
        info!("rm dependent!!");
        let mut guard = self.inner.write().unwrap();
        let res = guard
            .dependents
            .entry(dependency)
            .or_default()
            .remove(&dependent);
        info!("dependent rmed");
        res
    }

    pub fn set_dependent(&self, dependency: CardId, dependent: CardId) -> bool {
        self.inner
            .write()
            .unwrap()
            .dependents
            .entry(dependency)
            .or_default()
            .insert(dependent)
    }

    pub async fn remove_card(&self, card_id: CardId) {
        info!("cardprovider removing card: {card_id}");
        let _ = self.load(card_id).await; // ensure card is in cache first.

        // Other cards may depend on this now-deleted card, so we loop through them all to remove their dependency on it (if any).
        for card in self.load_all().await {
            info!("removing dependency for {}", card.id());
            let mut card = Arc::unwrap_or_clone(card);
            card.rm_dependency(card_id).await;
        }

        let (card, _revs, _deps) = self.remove_entry(card_id);
        let card = Arc::unwrap_or_clone(card.unwrap().card);
        self.provider.cards.delete_item(card.base).await;
        info!("done removing i guess");
    }

    fn remove_entry(&self, id: CardId) -> (Option<CardCache>, Option<RevCache>, Option<DepCache>) {
        info!("removing entry");
        let mut guard = self.inner.write().unwrap();
        let card = guard.cards.remove(&id);
        let rev = guard.reviews.remove(&id);
        let deps = guard.dependents.remove(&id);
        (card, rev, deps)
    }

    pub fn min_rec_recall_rate(
        &self,
        id: CardId,
    ) -> Pin<Box<dyn Future<Output = Result<RecallRate>> + '_>> {
        Box::pin(async move {
            trace!("card: {id} starting min rec recall calculation");
            let Some(card) = self.load(id).await else {
                eyre::bail!("couldnt find card: {id}");
            };

            let entry = self.load_cached_entry(id).await.unwrap();
            let recall_rate = card.recall_rate().unwrap_or_default();

            if let Some(recall) = entry.min_rec_recall {
                trace!("card: {id}: cached min rec recall: {recall}");
                return Ok(recall);
            }

            let dependencies = card.dependency_ids().await;

            if dependencies.is_empty() {
                trace!("card: {id}: no dependencies!");
                self.update_min_rec_recall(id, 1.0);
                Ok(recall_rate)
            } else {
                trace!("card: {id} traversing dependencies first: {dependencies:?}");
                let mut min_recall: RecallRate = 1.0;

                for dep in dependencies {
                    let rec = self.min_rec_recall_rate(dep).await?;
                    let Some(dep_rec) = self.load(dep).await else {
                        continue;
                    };
                    let dep_rec = dep_rec.recall_rate().unwrap_or_default();
                    min_recall = min_recall.min(rec);
                    min_recall = min_recall.min(dep_rec);
                    trace!("card: {id}: min recall updated: {min_recall}");
                }

                self.update_min_rec_recall(id, min_recall);
                trace!("card: {id}: new min rec recall: {min_recall}");
                Ok(min_recall)
            }
        })
    }

    fn update_min_rec_recall(&self, id: CardId, cached_recall: RecallRate) {
        trace!("card: {id}: caching min rec recall: {cached_recall}");
        let mut guard = self.inner.write().unwrap();
        guard.cards.get_mut(&id).unwrap().min_rec_recall = Some(cached_recall);
    }

    pub async fn load_all_card_ids(&self) -> Vec<CardId> {
        self.provider.cards.load_ids().await
    }

    pub async fn fill_cache(&self) {
        info!("1");
        let mut cards: HashMap<CardId, CardCache> = Default::default();
        let mut rev_caches: HashMap<CardId, RevCache> = Default::default();
        info!("loading cards");
        let raw_cards = self.provider.cards.load_all().await;
        info!("loading reviews");
        let mut reviews = self.provider.reviews.load_all().await;
        let audios = self.provider.audios.load_all().await;
        let mut metas = self.provider.metadata.load_all().await;
        let fetched = self.time_provider.current_time();

        for (id, card) in raw_cards {
            let front_audio = match card.front_audio {
                Some(id) => audios.get(&id).cloned(), // cloned not removed cause different cards can use same audio
                None => None,
            };

            let back_audio = match card.front_audio {
                Some(id) => audios.get(&id).cloned(),
                None => None,
            };

            let rev = reviews.remove(&id).unwrap_or_else(|| History::new(id));
            let meta = metas.remove(&id).unwrap_or_else(|| Metadata::new(id));
            let card = Card::from_parts(
                card,
                rev.clone(),
                meta,
                self.clone(),
                self.recaller.clone(),
                front_audio,
                back_audio,
            );
            let card = Arc::new(card);
            self.update_dependents(card.clone()).await;

            let reventry = RevCache {
                fetched,
                review: rev,
            };

            let entry = CardCache {
                fetched,
                card,
                min_rec_recall: None,
            };

            rev_caches.insert(entry.card.id(), reventry);
            cards.insert(entry.card.id(), entry);
        }

        let mut guard = self.inner.write().unwrap();
        guard.cards = cards;
        guard.reviews = rev_caches;
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

    pub async fn dependents(&self, id: CardId) -> BTreeSet<Arc<Card>> {
        trace!("dependents of: {}", id);
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

    pub async fn load(&self, id: CardId) -> Option<Arc<Card>> {
        trace!("loading card for id: {}", id);
        if let (Some(card), Some(_), Some(_)) = (
            self.load_cached_card(id).await,
            self.load_cached_reviews(id).await,
            self.load_cached_meta(id).await,
        ) {
            trace!("cache hit for id: {}", id);
            Some(card)
        } else {
            trace!("cache miss for id: {}", id);
            self.fresh_load(id).await
        }
    }

    pub async fn load_reviews(&self, id: CardId) -> History {
        self.provider
            .reviews
            .load_item(id)
            .await
            .unwrap_or_else(|| History::new(id))
    }

    pub async fn save_reviews(&self, reviews: History) {
        self.provider.reviews.save_item(reviews).await;
    }

    pub async fn save_basecard(&self, card: BaseCard) -> Arc<Card> {
        let id = card.id();
        self.provider.cards.update_item(card).await;
        self.invalidate_card(id).await;
        self.load(id).await.unwrap()
    }

    pub async fn save_card(&self, card: Card) {
        let id = card.id();
        self.update_cache(Arc::new(card.clone()));
        self.provider.metadata.save_item(card.meta()).await;
        let mut invalidated_indices = BTreeSet::new();

        if let Some(old_base) = self.provider.cards.load_item(card.id()).await {
            for idx in old_base.indices() {
                invalidated_indices.insert(idx);
            }
        }

        self.provider.cards.update_item(card.base).await;

        if let Some(new_base) = self.provider.cards.load_item(id).await {
            for idx in new_base.indices() {
                invalidated_indices.insert(idx);
            }
        }

        for idx in invalidated_indices {
            self.indices.write().unwrap().remove(&idx);
        }
    }

    pub async fn cache_ascii_indices(&self) {
        fn generate_ascii_bigrams() -> Vec<String> {
            let mut bigrams = Vec::with_capacity(26 * 26);

            for first in b'a'..=b'z' {
                for second in b'a'..=b'z' {
                    bigrams.push(format!("{}{}", first as char, second as char));
                }
            }

            bigrams
        }

        for bigram in generate_ascii_bigrams() {
            self.get_indices(bigram).await;
        }
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
                metadata: Default::default(),
            })),
            time_provider,
            provider,
            recaller,
            check_modified: false,
            indices: Default::default(),
        }
    }

    async fn load_uncached(&self, id: CardId) -> Option<Card> {
        trace!("load uncached");
        let raw_card = self.provider.cards.load_item(id).await?;

        let reviews = self
            .provider
            .reviews
            .load_item(id)
            .await
            .unwrap_or_else(|| History::new(id));

        let meta = self
            .provider
            .metadata
            .load_item(id)
            .await
            .unwrap_or_else(|| Metadata::new(id));

        let front_audio = match raw_card.front_audio {
            Some(id) => self.provider.audios.load_item(id).await,
            None => None,
        };
        let back_audio = match raw_card.back_audio {
            Some(id) => self.provider.audios.load_item(id).await,
            None => None,
        };

        let card = Card::from_parts(
            raw_card,
            reviews,
            meta,
            self.clone(),
            self.recaller.clone(),
            front_audio,
            back_audio,
        );

        Some(card)
    }

    async fn load_cached_meta(&self, id: CardId) -> Option<Metadata> {
        self.inner.read().unwrap().metadata.get(&id).cloned()
    }

    async fn load_cached_reviews(&self, id: CardId) -> Option<History> {
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
            let last_modified = cached.review.last_modified();
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

    async fn update_dependents(&self, card: Arc<Card>) {
        trace!("updating cache dependents");
        let mut guard = self.inner.write().unwrap();
        for dep in card.dependency_ids().await {
            guard.dependents.entry(dep).or_default().insert(card.id());
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

    async fn load_cached_card(&self, id: CardId) -> Option<Arc<Card>> {
        let cached = self.load_cached_entry(id).await?;

        if self.check_modified {
            let last_modified = cached.card.last_modified();
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

    fn update_cache(&self, card: Arc<Card>) {
        trace!("updating cache for card: {}", card.id());
        let now = self.time_provider.current_time();
        let mut guard = self.inner.write().unwrap();
        let id = card.id();

        let cached_meta = card.meta();

        let cached_reviews = RevCache {
            fetched: now,
            review: card.history().clone(),
        };
        let cached_card = CardCache {
            fetched: now,
            card,
            min_rec_recall: None,
        };

        guard.cards.insert(id, cached_card);
        guard.reviews.insert(id, cached_reviews);
        guard.metadata.insert(id, cached_meta);
    }

    async fn fresh_load(&self, id: CardId) -> Option<Arc<Card>> {
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
    metadata: HashMap<CardId, Metadata>,
}

#[derive(Clone, Debug)]
struct CardCache {
    fetched: Duration,
    card: Arc<Card>,
    min_rec_recall: Option<RecallRate>,
}

struct RevCache {
    fetched: Duration,
    review: History,
}

type DepCache = HashSet<CardId>;
