use std::{
    collections::{BTreeSet, HashMap},
    fmt::Debug,
    sync::{Arc, RwLock},
};

use dioxus_logger::tracing::{info, trace};
use tracing::warn;

use crate::{
    card::{BaseCard, CardId}, metadata::Metadata, recall_rate::History, Card, Provider, Recaller, TimeGetter
};

#[derive(Clone)]
pub struct CardProvider {
    cards: Arc<RwLock<HashMap<CardId, Arc<Card>>>>,
    pub providers: Provider,
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
    pub async fn remove_card(&self, card_id: CardId) {
        let card = self.providers.cards.load_item(card_id).await.unwrap();
        for card in self.load_all().await {
            info!("removing dependency for {}", card.id());
            let mut card = Arc::unwrap_or_clone(card);
            card.rm_dependency(card_id).await;
        }

        self.providers.cards.delete_item(card).await;
        info!("done removing i guess");
    }

    pub async fn load_all_card_ids(&self) -> Vec<CardId> {
        self.providers.cards.load_ids().await
    }

    pub async fn refresh_cache(&self) {
        info!("starting cache refresh... might take a while..");
        let cards = self.providers.cards.load_all().await;

        self.providers.indices.refresh(self, cards.values()).await;
        self.providers.dependents.refresh(cards.values()).await;

        info!("done with cache refresh!");
    }

    /// Checks that cache of given card is solid.
    async fn check_cache(&self, id: CardId) -> bool {
        let base_card = self.providers.cards.load_item(id).await.unwrap();

        for dependency in base_card.dependencies().await {
            if !self.providers.dependents.load(dependency).await.contains(&id) {
                warn!("card: {} has {} as dependency but it was not present in the dependents cache", id, dependency);
                return false;
            }
        }

        for bigram in base_card.bigrams(&self.clone()).await {
            if !self.providers.indices.load(bigram).await.contains(&id) {
                warn!("card: {} has {:?} bigram but it was not present in the indices", id, bigram);
                return false;
            }
        }

        true
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
        self.providers.dependents.load(id).await
    }


    pub async fn load(&self, id: CardId) -> Option<Arc<Card>> {
        if let Some(card) = self.cards.read().unwrap().get(&id).cloned() {
            if !self.check_cache(id).await {
                self.refresh_cache().await;
            }
            return Some(card);
        }

        let base = self.providers.cards.load_item(id).await?;
        let history = match self.providers.reviews.load_item(id).await {
            Some(revs) => revs,
            None => History::new(id),
        };
        let metadata = match self.providers.metadata.load_item(id).await {
            Some(meta) => meta,
            None => Metadata::new(id),
        };
        
        let front_audio = match base.front_audio {
            Some(audio) => Some(self.providers.audios.load_item(audio).await.unwrap()),
            None => None,
        };
        let back_audio = match base.back_audio {
            Some(audio) => Some(self.providers.audios.load_item(audio).await.unwrap()),
            None => None,
        };


        let card = Arc::new(Card::from_parts(base, history, metadata, self.clone(), self.recaller.clone(), front_audio, back_audio));

        self.cards.write().unwrap().insert(id, card.clone());

        if !self.check_cache(id).await {
            self.refresh_cache().await;
        }

        Some(card)
    }


    pub fn invalidate_card(&self, id: CardId) -> Option<Arc<Card>>{
        self.cards.write().unwrap().remove(&id)
    }

    pub async fn save_basecard(&self, new_card: BaseCard) -> Arc<Card> {
        let old_card = self.providers.cards.load_item(new_card.id).await;
        self.providers.dependents.update(old_card.as_ref(), &new_card).await;
        self.providers.indices.update(self, old_card.as_ref(), &new_card).await;
        self.invalidate_card(new_card.id);
        self.load(new_card.id).await.unwrap()
    }

    pub fn time_provider(&self) -> TimeGetter {
        self.time_provider.clone()
    }

    pub fn new(provider: Provider, time_provider: TimeGetter, recaller: Recaller) -> Self {
        Self {
            cards: Default::default(),
            time_provider,
            providers: provider,
            recaller,
        }
    }
}
