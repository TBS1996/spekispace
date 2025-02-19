use std::{collections::{BTreeMap, HashMap}, fmt::Debug, sync::{Arc, RwLock}, time::Duration};

use audio::Audio;
use card::{BackSide, BaseCard, CardId, RecallRate};
use card_provider::CardProvider;
use cardfilter::{CardFilter, FilterItem};
use collection::{Collection, CollectionId};
use dependents::{Dependents, DependentsProvider};
use dioxus_logger::tracing::info;
use eyre::Result;
use index::{Index, IndexProvider};
use metadata::Metadata;
use recall_rate::History;
use speki_dto::{Indexable, Item, Record, SpekiProvider, TimeProvider};
use tracing::trace;

mod attribute;
pub mod audio;
pub mod card;
mod card_provider;
pub mod cardfilter;
pub mod collection;
mod common;
pub mod metadata;
pub mod recall_rate;
pub mod dependents;
pub mod index;
pub mod ledger;

pub use attribute::{Attribute, AttributeDTO, AttributeId};
pub use card::{
    AttributeCard, Card, CardTrait, CardType, ClassCard, EventCard, InstanceCard, NormalCard,
    StatementCard, UnfinishedCard,
};
pub use common::current_time;
pub use omtrent::TimeStamp;
pub use recall_rate::SimpleRecall;

pub trait RecallCalc {
    fn recall_rate(&self, reviews: &History, current_unix: Duration) -> Option<RecallRate>;
}

#[derive(Clone)]
pub struct CollectionProvider {
   inner: Arc<Box<dyn SpekiProvider<Collection>>>,
}

impl CollectionProvider {
    pub fn new(inner: Arc<Box<dyn SpekiProvider<Collection>>>) -> Self {
        Self {
            inner
        }
    }
    pub async fn save(&self, collection: Collection) {
        self.inner.save_item(collection).await
    }

    pub async fn load(&self, id: CollectionId) -> Option<Collection> {
        if let Some(col) = self.inner.load_item(id).await {
            Some(col)
        } else {
            None
        }
    }

    pub async fn load_all(&self) -> HashMap<CollectionId, Collection> {
        self.inner.load_all().await
    }

    pub async fn delete(&self, item: Collection) {
        self.inner.delete_item(item).await
    }
}

#[derive(Clone)]
pub struct Provider {
    pub cards: Arc<Box<dyn SpekiProvider<BaseCard>>>,
    pub reviews: Arc<Box<dyn SpekiProvider<History>>>,
    pub attrs: Arc<Box<dyn SpekiProvider<AttributeDTO>>>,
    pub collections: CollectionProvider,
    pub metadata: Arc<Box<dyn SpekiProvider<Metadata>>>,
    pub cardfilter: Arc<Box<dyn SpekiProvider<FilterItem>>>,
    pub audios: Arc<Box<dyn SpekiProvider<Audio>>>,
    pub dependents: DependentsProvider,
    pub indices: IndexProvider,
}

#[derive(Clone)]
pub struct MemProvider<T: Item + Send + 'static>{
    provider: Arc<Box<dyn SpekiProvider<T> + Send >>, 
    cache: Arc<RwLock<BTreeMap<T::Key, T>>>,
}

impl<T: Item + Send + 'static> MemProvider<T> {
    pub fn new(provider: Arc<Box<dyn SpekiProvider<T> + Send >>) -> Self {
        Self {
            provider,
            cache: Default::default(),
        }
    }
}

#[async_trait::async_trait(?Send)]
impl<T: Item + Send + Sync + 'static> SpekiProvider<T> for MemProvider<T> {
    async fn current_time(&self) -> Duration {
        self.provider.current_time().await
    }

    async fn load_record(&self, id: T::Key) -> Option<Record> {
        trace!("mem load record: {id:?}");
        self.provider.load_record(id).await
    }

    async fn load_all_records(&self) -> HashMap<T::Key, Record> {
        trace!("ty: {} loading all records", T::identifier());
        self.provider.load_all_records().await
    }

    async fn save_record(&self, record: Record) {
        trace!("ty: {} save record", T::identifier());
        self.provider.save_record(record).await;
    }

    async fn save_records(&self, records: Vec<Record>) {
        for record in records {
            self.save_record(record).await;
        }
    }

    async fn load_ids(&self) -> Vec<T::Key> {
        trace!("ty: {} loading ids", T::identifier());
        self.load_all_records().await.into_keys().collect()
    }

    async fn load_item(&self, id: T::Key) -> Option<T> {
        trace!("ty: {} loading id {id:?}", T::identifier());
        if let Some(item) = self.cache.read().unwrap().get(&id).cloned() {
        trace!("ty: {} loading id {id:?} cache hit", T::identifier());
            return Some(item);
        };
        trace!("ty: {} loading id {id:?} cache miss", T::identifier());

        if let Some(item) = self.provider.load_item(id).await {
            trace!("ty: {} loading id {id:?} loaded item from inner provider", T::identifier());
            self.cache.write().unwrap().insert(id, item.clone());
            Some(item)
        } else {
            trace!("ty: {} loading id {id:?} did not load item from inner provider", T::identifier());
            None
        }

    }

    async fn save_item(&self, mut item: T) {
        item.set_last_modified(self.current_time().await);
        item.set_local_source();
        self.cache.write().unwrap().insert(item.id(), item.clone());
        let record: Record = item.into();
        self.save_record(record).await;
    }
}

pub type Recaller = Arc<Box<dyn RecallCalc + Send>>;
pub type TimeGetter = Arc<Box<dyn TimeProvider + Send>>;

pub struct App {
    pub provider: Provider,
    pub card_provider: CardProvider,
    pub time_provider: TimeGetter,
    pub recaller: Recaller,
}

impl Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "app!")
    }
}

impl App {
    pub fn new<A, B, C, D, E, F, G, H, I, J, K>(
        recall_calc: A,
        time_provider: B,
        card_provider: C,
        history_provider: D,
        attr_provider: E,
        collections_provider: F,
        meta_provider: G,
        filter_provider: H,
        audio_provider: I,
        dependents_provider: J,
        index_provider: K,
    ) -> Self
    where
        A: RecallCalc + 'static + Send,
        B: TimeProvider + 'static + Send,
        C: Indexable<BaseCard> + 'static + Send,
        D: SpekiProvider<History> + 'static + Send,
        E: SpekiProvider<AttributeDTO> + 'static + Send,
        F: SpekiProvider<Collection> + 'static + Send,
        G: SpekiProvider<Metadata> + 'static + Send,
        H: SpekiProvider<FilterItem> + 'static + Send,
        I: SpekiProvider<Audio> + 'static + Send,
        J: SpekiProvider<Dependents> + 'static + Send,
        K: SpekiProvider<Index> + 'static + Send,
    {
        info!("initialtize app");

        let time_provider: TimeGetter = Arc::new(Box::new(time_provider));
        let recaller: Recaller = Arc::new(Box::new(recall_calc));

        let provider = Provider {
            cards: Arc::new(Box::new(MemProvider::new(Arc::new(Box::new(card_provider))))),
            reviews: Arc::new(Box::new(MemProvider::new(Arc::new(Box::new(history_provider))))),
            attrs: Arc::new(Box::new(MemProvider::new(Arc::new(Box::new(attr_provider))))),
            collections: CollectionProvider {
                inner: Arc::new(Box::new(MemProvider::new(Arc::new(Box::new(collections_provider))))),
            },
            metadata: Arc::new(Box::new(MemProvider::new(Arc::new(Box::new(meta_provider))))),
            cardfilter: Arc::new(Box::new(MemProvider::new(Arc::new(Box::new(filter_provider))))),
            audios: Arc::new(Box::new(MemProvider::new(Arc::new(Box::new(audio_provider))))),
            dependents: DependentsProvider::new(Arc::new(Box::new(MemProvider::new(Arc::new(Box::new(dependents_provider)))))),
            indices: IndexProvider::new(Arc::new(Box::new(MemProvider::new(Arc::new(Box::new(index_provider)))))),
            
        };

        let card_provider =
            CardProvider::new(provider.clone(), time_provider.clone(), recaller.clone());

        Self {
            provider,
            card_provider,
            time_provider,
            recaller,
        }
    }

    pub async fn index_all(&self) {
        //self.provider.cards.index_all().await;
        self.card_provider.refresh_cache().await;
    }

    pub fn card_provider(&self) -> CardProvider {
        self.card_provider.clone()
    }

    pub async fn fill_cache(&self) {
        info!("filling cache");
        let start = self.time_provider.current_time();
        let elapsed = self.time_provider.current_time() - start;
        info!("cache filled in {:.4} seconds!", elapsed.as_secs_f32());
    }

    pub async fn fill_index_cache(&self) {
        info!("filling ascii bigram indices");
        let start = self.time_provider.current_time();
        //self.card_provider.cache_ascii_indices().await;
        let elapsed = self.time_provider.current_time() - start;
        info!(
            "ascii bigram indices filled in {:.4} seconds!",
            elapsed.as_secs_f32()
        );
    }

    pub async fn load_all_cards(&self) -> Vec<Arc<Card>> {
        self.card_provider.load_all().await
    }

    pub async fn load_card(&self, id: CardId) -> Option<Card> {
        trace!("loading card: {id}");
        let card = self.card_provider.load(id).await;
        trace!("card loaded i guess: {card:?}");
        Some(Arc::unwrap_or_clone(card?))
    }

    pub async fn load_cards(&self) -> Vec<CardId> {
        self.card_provider.load_all_card_ids().await
    }

    pub async fn cards_filtered(&self, filter: CardFilter) -> Vec<Arc<Card>> {
        let cards = self.load_all_cards().await;
        let mut ids = vec![];

        for card in cards {
            if filter.filter(card.clone()).await {
                ids.push(card);
            }
        }
        ids
    }

    pub async fn add_class(
        &self,
        front: String,
        back: impl Into<BackSide>,
        parent_class: Option<CardId>,
    ) -> CardId {
        let back = back.into();
        let data = ClassCard {
            name: front,
            back,
            parent_class,
        };

        let base = BaseCard::new(data);
        self.card_provider().save_basecard(base).await.id()
    }

    pub async fn add_instance(
        &self,
        front: String,
        back: Option<impl Into<BackSide>>,
        class: CardId,
    ) -> CardId {
        let back = back.map(|back| back.into());
        let data = InstanceCard {
            name: front,
            back,
            class,
        };
        let base = BaseCard::new(data);
        self.card_provider().save_basecard(base).await.id()
    }

    pub async fn add_card_with_id(&self, front: String, back: impl Into<BackSide>, id: CardId) {
        let back = back.into();
        let data = NormalCard { front, back };
        let card = BaseCard::new_with_id(id, data);
        self.provider.cards.save_item(card).await;
    }

    pub async fn add_card(&self, front: String, back: impl Into<BackSide>) -> CardId {
        let back = back.into();
        let data = NormalCard { front, back };
        let base = BaseCard::new(data);
        self.card_provider().save_basecard(base).await.id()
    }

    pub async fn add_unfinished(&self, front: String) -> CardId {
        let data = UnfinishedCard { front };
        let base = BaseCard::new(data);
        self.card_provider().save_basecard(base).await.id()
    }

    pub async fn set_class(&self, card_id: CardId, class: CardId) -> Result<()> {
        let card = self.card_provider.load(card_id).await.unwrap();

        let instance = InstanceCard {
            name: card.print().await,
            back: card.back_side().map(ToOwned::to_owned),
            class,
        };
        Arc::unwrap_or_clone(card).into_type(instance).await;
        Ok(())
    }

    pub async fn load_class_cards(&self) -> Vec<Arc<Card>> {
        self.load_all_cards()
            .await
            .into_iter()
            .filter(|card| card.is_class())
            .collect()
    }
}

pub async fn as_graph(app: &App) -> String {
    graphviz::export(app).await
}

mod graphviz {
    use std::collections::BTreeSet;

    use super::*;

    pub async fn export(app: &App) -> String {
        let mut dot = String::from("digraph G {\nranksep=2.0;\nrankdir=BT;\n");
        let mut relations = BTreeSet::default();
        let cards = app.load_all_cards().await;

        for card in cards {
            let label = card
                .print()
                .await
                .to_string()
                .replace(")", "")
                .replace("(", "")
                .replace("\"", "");

            let color = match card.recall_rate() {
                _ if !card.is_finished() => yellow_color(),
                Some(rate) => rate_to_color(rate as f64 * 100.),
                None => cyan_color(),
            };

            match card.recall_rate() {
                Some(rate) => {
                    let recall_rate = rate * 100.;
                    let maturity = card.maturity().unwrap_or_default();
                    dot.push_str(&format!(
                        "    \"{}\" [label=\"{} ({:.0}%/{:.0}d)\", style=filled, fillcolor=\"{}\"];\n",
                        card.id(),
                        label,
                        recall_rate,
                        maturity,
                        color
                    ));
                }
                None => {
                    dot.push_str(&format!(
                        "    \"{}\" [label=\"{} \", style=filled, fillcolor=\"{}\"];\n",
                        card.id(),
                        label,
                        color
                    ));
                }
            }

            // Create edges for dependencies, also enclosing IDs in quotes
            for child_id in card.dependencies().await {
                relations.insert(format!("    \"{}\" -> \"{}\";\n", card.id(), child_id));
            }
        }

        for rel in relations {
            dot.push_str(&rel);
        }

        dot.push_str("}\n");
        dot
    }

    // Convert recall rate to a color, from red to green
    fn rate_to_color(rate: f64) -> String {
        let red = ((1.0 - rate / 100.0) * 255.0) as u8;
        let green = (rate / 100.0 * 255.0) as u8;
        format!("#{:02X}{:02X}00", red, green) // RGB color in hex
    }

    fn cyan_color() -> String {
        String::from("#00FFFF")
    }

    fn yellow_color() -> String {
        String::from("#FFFF00")
    }
}
