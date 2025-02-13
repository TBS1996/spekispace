
use std::{collections::{BTreeMap, BTreeSet}, sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use speki_dto::{Item, ModifiedSource, SpekiProvider};
use tracing::info;

use crate::{card::{BaseCard, CardId}, card_provider::CardProvider};

#[derive(Clone)]
pub struct IndexProvider {
    inner: Arc<Box<dyn SpekiProvider<Index>>>,
}

impl IndexProvider {
    pub fn new(inner: Arc<Box<dyn SpekiProvider<Index>>>) -> Self {
        Self {
            inner
        }
    }

    pub async fn load(&self, bigram: Bigram) -> BTreeSet<CardId>{
        self.inner.load_item(bigram).await.map(|i|i.deps).unwrap_or_default()
    }

    pub async fn check(&self, provider: &CardProvider, card: &BaseCard) -> bool{
        for bigram in card.bigrams(provider).await {
            if !self.load(bigram).await.contains(&card.id) {
                tracing::warn!("card: {} has {:?} bigram but it was not present in the indices", card.id, bigram);
                return false;
            }
        }

        true
    }

    pub async fn refresh(&self, provider: &CardProvider, cards: impl IntoIterator<Item=&BaseCard>) {
        info!("refreshing indices..");
        let mut indices: BTreeMap<Bigram, BTreeSet<CardId>> = Default::default();
        for card in cards {
            for bigram in card.bigrams(provider).await {
                indices.entry(bigram).or_default().insert(card.id);
            }
        }
        for (bigram, indices) in indices {
            self.inner.save_item(Index::new(bigram, indices, Default::default())).await;
        }
        info!("done refreshing indices..");
    }

    pub async fn update(&self, provider: &CardProvider, old_card: Option<&BaseCard>, new_card: &BaseCard) {
        let id = new_card.id;

        let old_indices = match old_card {
            Some(card) => card.bigrams(provider).await,
            None => Default::default(),
        };

        for idx in old_indices{
            if let Some(mut index) = self.inner.load_item(idx).await {
                index.deps.remove(&id);
                self.inner.save_item(index).await;
            }
        }


        for idx in new_card.bigrams(provider).await {
            if let Some(mut index) = self.inner.load_item(idx).await {
                index.deps.insert(id);
                self.inner.save_item(index).await;
            }
        }
    }
}


#[derive(Clone, Copy, PartialEq, PartialOrd, Debug, Serialize, Deserialize, Hash, Ord, Eq)]
pub struct Bigram([char;2]);

impl Bigram {
    pub fn new(a: char, b: char) -> Self {
        Self([a, b])
    }
}

impl ToString for Bigram{
    fn to_string(&self) -> String {
        serde_json::to_string(&self.0).unwrap()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Index {
    id: Bigram,
    pub deps: BTreeSet<CardId>,
    source: ModifiedSource,
    last_modified: Duration,
}

impl Index {
    pub fn new(id: Bigram, deps: BTreeSet<CardId>, current_time: Duration) -> Self {
        Self {
            id,
            deps,
            source: Default::default(),
            last_modified: current_time,
        }
    }
}


impl Item for Index {
    type PreviousVersion = Index;
    type Key = Bigram;

    fn deleted(&self) -> bool {
        false
    }

    fn set_delete(&mut self) {
        panic!("don't delete dependents cache!")
    }

    fn set_last_modified(&mut self, time: std::time::Duration) {
        self.last_modified = time;
    }

    fn last_modified(&self) -> std::time::Duration {
        self.last_modified
    }

    fn id(&self) -> Self::Key{
        self.id
    }

    fn identifier() -> &'static str {
        "indices"
    }

    fn source(&self) -> ModifiedSource {
        self.source
    }

    fn set_source(&mut self, source: ModifiedSource) {
        self.source = source;
    }
}