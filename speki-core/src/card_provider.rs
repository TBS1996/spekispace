use crate::{
    audio::Audio,
    card::CardId,
    ledger::{CardAction, CardEvent},
    metadata::Metadata,
    recall_rate::History,
    Card, Provider, Recaller, TimeGetter,
};
use dioxus_logger::tracing::{info, trace};
use std::{collections::BTreeSet, fmt::Debug, sync::Arc};

#[derive(Clone)]
pub struct CardProvider {
    //cards: Arc<RwLock<HashMap<CardId, Arc<Card>>>>,
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
        let event = CardEvent::new(card_id, CardAction::DeleteCard);
        self.providers.run_event(event);
    }

    pub fn load_all_card_ids(&self) -> Vec<CardId> {
        info!("x1");
        self.providers
            .cards
            .load_ids()
            .into_iter()
            .map(|id| id.parse().unwrap())
            .collect()
    }

    pub async fn load_all(&self) -> Vec<Arc<Card>> {
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
            .get_dependents(id)
            .into_iter()
            .map(|x| x.parse().unwrap())
            .collect()
    }

    pub fn load(&self, id: CardId) -> Option<Arc<Card>> {
        //if let Some(card) = self.cards.read().unwrap().get(&id).cloned() {
        //    return Some(card);
        //}

        let base = self.providers.cards.load(id)?;
        let history = match self.providers.reviews.load(id) {
            Some(revs) => revs,
            None => History::new(id),
        };
        let metadata = match self.providers.metadata.load(id) {
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

        //self.cards.write().unwrap().insert(id, card.clone());

        Some(card)
    }

    pub fn time_provider(&self) -> TimeGetter {
        self.time_provider.clone()
    }

    pub fn new(provider: Provider, time_provider: TimeGetter, recaller: Recaller) -> Self {
        Self {
            time_provider,
            recaller,
            providers: provider,
        }
    }
}
