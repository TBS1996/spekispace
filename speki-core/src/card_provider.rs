use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt::Debug,
    sync::{Arc, RwLock},
};

use dioxus_logger::tracing::{info, trace};
use tracing::warn;

use crate::{
    card::{BaseCard, CardId}, dependents::Dependents, index::Index, metadata::Metadata, recall_rate::History, Card, Provider, Recaller, TimeGetter
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

    async fn refresh_cache(&self) {
        info!("starting cache refresh... might take a while..");
        let cards = self.providers.cards.load_all().await;

        for (_, card) in &cards {
            for bigram in card.bigrams(self).await {
                let mut indices = match self.providers.indices.load_item(bigram).await {
                    Some(idx) => idx,
                    None => Index::new(bigram, Default::default(), self.time_provider.current_time()),
                };
                indices.deps.insert(card.id);
                self.providers.indices.save_item(indices).await;
            }
        }

        for (_, card) in &cards {
            for dependency in card.dependencies().await {
                let mut indices = match self.providers.dependents.load_item(dependency).await {
                    Some(idx) => idx,
                    None => Dependents::new(dependency, Default::default(), self.time_provider.current_time()),
                };
                indices.deps.insert(card.id);
                self.providers.dependents.save_item(indices).await;
            }
        }
        info!("done with cache refresh!");
    }

    /// Checks that cache of given card is solid.
    async fn check_cache(&self, id: CardId) -> bool {
        let base_card = self.providers.cards.load_item(id).await.unwrap();

        for dependency in base_card.dependencies().await {
            match self.providers.dependents.load_item(dependency).await {
                Some(dependents) => {
                    if !dependents.deps.contains(&id) {
                        warn!("card: {} has {} as dependency but it was not present in the dependents cache", id, dependency);
                        return false;
                    }
                },
                None => return false,
            }

        }

        for bigram in base_card.bigrams(&self.clone()).await {
            match self.providers.indices.load_item(bigram).await {
                Some(indices) => {
                    if !indices.deps.contains(&id) {
                        warn!("card: {} has {:?} bigram but it was not present in the indices", id, bigram);
                        return false;
                    }
                },
                None => return false,
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
        self.providers.dependents.load_item(id).await.map(|x|x.deps).unwrap_or_default()
    }

    pub async fn fill_dependents(&self) {
        info!("filling all dependents cache");

        let current_time = self.time_provider.current_time();
        let cards = self.load_all().await;
        let mut deps: BTreeMap<CardId, BTreeSet<CardId>> = BTreeMap::default();

        for card in cards {
            for dependency in card.dependencies().await {
                deps.entry(dependency).or_default().insert(card.id());
            }
        }

        for (id, deps) in deps {
            let dependents = Dependents::new(id, deps, current_time);
            self.providers.dependents.save_item(dependents).await;
        }
        info!("done filling all dependents cache");
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

    pub async fn update_indices(&self, old_card: Option<&BaseCard>, new_card: &BaseCard) {
        let id = new_card.id;

        let old_indices = match old_card {
            Some(card) => card.bigrams(&self.clone()).await,
            None => Default::default(),
        };

        for idx in old_indices{
            if let Some(mut index) = self.providers.indices.load_item(idx).await {
                index.deps.remove(&id);
                self.providers.indices.save_item(index).await;
            }
        }


        for idx in new_card.bigrams(&self.clone()).await {
            if let Some(mut index) = self.providers.indices.load_item(idx).await {
                index.deps.insert(id);
                self.providers.indices.save_item(index).await;
            }
        }
    }

    pub async fn update_dependents(&self, old_card: Option<&BaseCard>, new_card: &BaseCard) {
        let id = new_card.id;

        let new_dependencies = new_card.dependencies().await;
        let old_dependencies  = {
            if let Some(card)= 
            old_card {
                card.dependencies().await
            } else {
                Default::default()
            }
        };

        let removed_dependencies = old_dependencies.difference(&new_dependencies);
        let added_dependencies = new_dependencies.difference(&old_dependencies);

        for dependency in removed_dependencies {
            let mut dependents = if let Some(deps) = self.providers.dependents.load_item(*dependency).await  {
                deps
            } else {
                Dependents::new(*dependency, Default::default(), self.time_provider.current_time())
            };

            dependents.deps.remove(&id);
            self.providers.dependents.save_item(dependents).await;
        }

        for dependency in added_dependencies {
            let mut dependents = if let Some(deps) = self.providers.dependents.load_item(*dependency).await  {
                deps
            } else {
                Dependents::new(*dependency, Default::default(), self.time_provider.current_time())
            };

            dependents.deps.contains(&id);
            dependents.deps.insert(id);
            self.providers.dependents.save_item(dependents).await;
        }
    }

    pub fn invalidate_card(&self, id: CardId) -> Option<Arc<Card>>{
        self.cards.write().unwrap().remove(&id)
    }

    pub async fn save_basecard(&self, new_card: BaseCard) -> Arc<Card> {
        let old_card = self.providers.cards.load_item(new_card.id).await;
        self.update_dependents(old_card.as_ref(), &new_card).await;
        self.update_indices(old_card.as_ref(), &new_card).await;
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
