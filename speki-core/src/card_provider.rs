use crate::{
    audio::Audio,
    card::{CardId, RawCard},
    ledger::{CardEvent, MetaEvent},
    metadata::Metadata,
    recall_rate::{AvgRecall, History, ReviewEvent},
    set::{Set, SetEvent},
    Card, FsTime, Provider,
};
use dioxus_logger::tracing::{info, trace};
use ledgerstore::EventError;
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fmt::Debug,
    sync::{Arc, RwLock},
};

#[derive(Clone)]
pub struct CardProvider {
    pub providers: Provider,
    time_provider: FsTime,
    recaller: AvgRecall,
    cache: Arc<RwLock<HashMap<CardId, Arc<Card>>>>,
}

impl Debug for CardProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CardProvider")
            .field("inner", &":)")
            .finish()
    }
}

impl CardProvider {
    pub fn load_all_card_ids(&self) -> Vec<CardId> {
        info!("x1");
        self.providers.cards.load_ids().into_iter().collect()
    }

    pub fn duplicates(&self) -> HashSet<String> {
        info!("finding duplicates!");
        let mut cards: Vec<String> = self
            .load_all()
            .into_iter()
            .map(|c| c.display_card(true, true).to_lowercase())
            .collect();

        cards.sort();

        let mut duplicates: HashSet<String> = Default::default();

        let mut prev = String::new();
        for card in cards.into_iter() {
            if &card == &prev {
                duplicates.insert(card.clone());
            }

            prev = card;
        }

        duplicates
    }

    pub fn modify_set(&self, event: SetEvent) -> Result<(), EventError<Set>> {
        self.providers.sets.modify(event)
    }

    pub fn modify_metadata(&self, event: MetaEvent) -> Result<(), EventError<Metadata>> {
        let id = event.id();

        self.providers.metadata.modify(event)?;

        if let Some(id) = id {
            let mut guard = self.cache.write().unwrap();
            guard.remove(&id);

            for id in self.dependents(id) {
                guard.remove(&id);
            }
        }

        Ok(())
    }

    pub fn modify_review(&self, event: ReviewEvent) -> Result<(), EventError<History>> {
        let id = event.id();
        self.providers.reviews.modify(event)?;

        if let Some(id) = id {
            let mut guard = self.cache.write().unwrap();
            guard.remove(&id);

            /*
            for id in self.dependents(id) {
                guard.remove(&id);
            }
            */
        }

        Ok(())
    }

    pub fn modify_card(&self, event: CardEvent) -> Result<(), EventError<RawCard>> {
        let id = event.id();

        self.providers.cards.modify(event)?;

        if let Some(id) = id {
            let mut guard = self.cache.write().unwrap();
            guard.remove(&id);

            for id in self.dependents(id) {
                guard.remove(&id);
            }
        }

        Ok(())
    }

    pub fn load_all(&self) -> Vec<Arc<Card>> {
        info!("load all");
        let mut out: Vec<Arc<Card>> = vec![];
        let ids = self.load_all_card_ids();

        let mut guard = self.cache.write().unwrap();

        for id in ids {
            let card = self.load(id).unwrap();
            guard.insert(id, card.clone());
            out.push(card);
        }
        out
    }

    pub fn dependents(&self, id: CardId) -> BTreeSet<CardId> {
        trace!("dependents of: {}", id);

        self.providers
            .cards
            .dependents_recursive(id)
            .into_iter()
            .collect()
    }

    pub fn load(&self, id: CardId) -> Option<Arc<Card>> {
        if let Some(card) = self.cache.read().unwrap().get(&id).cloned() {
            return Some(card);
        }

        let (base, is_remote) = self.providers.cards.load_with_remote_info(id)?;
        let history = match self.providers.reviews.load(id) {
            Some(revs) => revs,
            None => Arc::new(History::new(id)),
        };
        let metadata = match self.providers.metadata.load(id) {
            Some(meta) => meta,
            None => Arc::new(Metadata::new(id)),
        };

        let front_audio: Option<Audio> = None;
        let back_audio: Option<Audio> = None;

        let card = Arc::new(Card::from_parts(
            base,
            is_remote,
            history,
            metadata,
            self.clone(),
            self.recaller.clone(),
            front_audio,
            back_audio,
        ));

        self.cache.write().unwrap().insert(id, card.clone());

        Some(card)
    }

    pub fn time_provider(&self) -> FsTime {
        self.time_provider.clone()
    }

    pub fn new(provider: Provider, time_provider: FsTime, recaller: AvgRecall) -> Self {
        Self {
            time_provider,
            recaller,
            providers: provider,
            cache: Default::default(),
        }
    }
}
