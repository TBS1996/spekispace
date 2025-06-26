use crate::{
    audio::Audio, card::CardId, metadata::Metadata, recall_rate::History, Card, FsTime, Provider,
    SimpleRecall,
};
use dioxus_logger::tracing::{info, trace};
use std::{collections::BTreeSet, fmt::Debug, sync::Arc};

#[derive(Clone)]
pub struct CardProvider {
    pub providers: Provider,
    time_provider: FsTime,
    recaller: SimpleRecall,
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

    pub fn load_all(&self) -> Vec<Arc<Card>> {
        info!("load all");
        let mut out: Vec<Arc<Card>> = vec![];
        let ids = self.load_all_card_ids();

        for id in ids {
            let card = self.load(id).unwrap();
            out.push(card);
        }
        out
    }

    pub fn dependents(&self, id: CardId) -> BTreeSet<CardId> {
        trace!("dependents of: {}", id);

        self.providers
            .cards
            .all_dependents(id)
            .into_iter()
            .collect()
    }

    pub fn load(&self, id: CardId) -> Option<Arc<Card>> {
        let base = self.providers.cards.load(id);
        let history = match self.providers.reviews.try_load(id) {
            Some(revs) => revs,
            None => History::new(id),
        };
        let metadata = match self.providers.metadata.try_load(id) {
            Some(meta) => meta,
            None => Metadata::new(id),
        };

        let front_audio: Option<Audio> = None;
        let back_audio: Option<Audio> = None;

        let card = Arc::new(Card::from_parts(
            base,
            history,
            metadata,
            self.clone(),
            self.recaller.clone(),
            front_audio,
            back_audio,
        ));

        Some(card)
    }

    pub fn time_provider(&self) -> FsTime {
        self.time_provider.clone()
    }

    pub fn new(provider: Provider, time_provider: FsTime, recaller: SimpleRecall) -> Self {
        Self {
            time_provider,
            recaller,
            providers: provider,
        }
    }
}
