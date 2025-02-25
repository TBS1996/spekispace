use std::{collections::{HashMap, HashSet}, fmt::Debug, hash, sync::{Arc, RwLock}, time::Duration};
use card::{BackSide, BaseCard, CType, CardId, RecallRate};
use card_provider::CardProvider;
use cardfilter::{CardFilter};
use collection::{Collection, CollectionId};
use dioxus_logger::tracing::info;
use eyre::Result;
use ledger::{check_compose, decompose, CardAction, CardEvent, CollectionEvent, Event, MetaEvent};
use metadata::Metadata;
use recall_rate::{History, ReviewEvent};
use serde::{de::DeserializeOwned, Serialize};
use speki_dto::{LedgerEntry, LedgerEvent, RunLedger, SpekiProvider, TimeProvider};
use tracing::trace;

pub mod attribute;
pub mod audio;
pub mod card;
pub mod card_provider;
pub mod cardfilter;
pub mod collection;
mod common;
pub mod metadata;
pub mod recall_rate;
pub mod ledger;

pub use attribute::{Attribute, AttributeDTO, AttributeId};
pub use card::{
    AttributeCard, Card, CardTrait, CardType, ClassCard, EventCard, InstanceCard, NormalCard,
    StatementCard, UnfinishedCard,
};
pub use common::current_time;
pub use omtrent::TimeStamp;
pub use recall_rate::SimpleRecall;


#[derive(Clone, PartialEq, PartialOrd, Hash, Eq, Debug)]
pub enum CacheKey {
    Dependent(CardId),
    Bigram([char;2]),
    Suspended(bool),
    CardType(CType),
    Instance(CardId),
    BackRef(CardId),
    SubClass(CardId),
    AttrId(AttributeId),
    AttrClass(CardId),
}

impl CacheKey {
    pub fn to_string(&self) -> String {
        match self {
            CacheKey::Dependent(id) => format!("dependents:{id}"),
            CacheKey::Bigram([a, b]) => format!("bigram:{a}{b}"),
            CacheKey::Suspended(flag) => format!("suspended:{flag}"),
            CacheKey::CardType(cty) => format!("type:{:?}", cty),
            CacheKey::Instance(id) => format!("instance:{id}"),
            CacheKey::BackRef(id) => format!("backref:{id}"),
            CacheKey::SubClass(id) => format!("subclass:{id}"),
            CacheKey::AttrId(id) => format!("attrid:{id}"),
            CacheKey::AttrClass(id) => format!("attrclass:{id}"),
        }
    }
}

pub trait RecallCalc {
    fn recall_rate(&self, reviews: &History, current_unix: Duration) -> Option<RecallRate>;
}

#[derive(Clone)]
pub struct CollectionProvider {
   inner: Arc<Box<dyn SpekiProvider<Collection, CollectionEvent>>>,
}

impl CollectionProvider {
    pub fn new(inner: Arc<Box<dyn SpekiProvider<Collection, CollectionEvent>>>) -> Self {
        Self {
            inner
        }
    }
    pub async fn save(&self, collection: Collection) {
        self.inner.save_item(collection).await
    }

    pub async fn load(&self, id: CollectionId) -> Option<Collection> {
        if let Some(col) = self.inner.load_item(&id.to_string()).await {
            Some(col)
        } else {
            None
        }
    }

    pub async fn load_all(&self) -> HashMap<CollectionId, Collection> {
        self.inner.load_all().await.into_iter().map(|(key, val)| (key.parse().unwrap(), val)).collect()
    }
}

#[derive(Clone)]
pub struct Provider {
    pub cards: Arc<Box<dyn SpekiProvider<BaseCard, CardEvent>>>,
    pub reviews: Arc<Box<dyn SpekiProvider<History, ReviewEvent>>>,
    pub collections: CollectionProvider,
    pub metadata: Arc<Box<dyn SpekiProvider<Metadata, MetaEvent>>>,
    pub time: TimeGetter,
}

impl Provider {
    async fn run_card_event(&self, event: CardEvent) {
        self.cards.save_and_run(event, self.time.current_time()).await;
    }

    pub async fn check_decompose_lol(&self) {
        for (_, card) in self.cards.load_all().await{
            check_compose(card);
        }
    }

    pub async fn decompose_save_card_ledger(&self) {}

    pub async fn derive_card_ledger_from_state(&self) -> Vec<CardEvent>{
        let mut actions: Vec<CardEvent> = vec![];

        for (_, card) in self.cards.load_all().await {
            for action in decompose(&card) {
                actions.push(action);
            }
        }

        for i in 0..actions.len() {
            if matches!(&actions[i].action, CardAction::UpsertCard {..}) {
                let action = actions.remove(i);
                actions.insert(0, action);
            }
        }

        actions
    }

    pub async fn run_event(&self, event: impl Into<Event>) {
        match event.into() {
            Event::Meta(event) => self.run_meta_event(event).await,
            Event::History(event) => self.run_history_event(event).await,
            Event::Card(event) => self.run_card_event(event).await,
            Event::Collection(col) => self.collections.inner.save_and_run(col, self.time.current_time()).await,
        }
    }

    async fn run_history_event(&self, event: ReviewEvent) {
        let time = event.timestamp;
        self.reviews.save_and_run(event, time).await;
    }

    async fn run_meta_event(&self, event: MetaEvent) {
        self.metadata.save_and_run(event, self.time.current_time()).await;
    }
}


#[derive(Clone)]
pub struct PureMem<L: LedgerEvent>{
    time: TimeGetter,
    state: Arc<RwLock<HashMap<String, String>>>,
    ledger: Arc<RwLock<HashMap<u64, LedgerEntry<L>>>>,
    cache: Arc<RwLock<HashMap<String, HashSet<String>>>>,
}

impl<E: LedgerEvent + Serialize + DeserializeOwned + Clone + 'static> PureMem<E> {
    pub fn new(time: TimeGetter) -> Self {
        Self {
            time,
            state: Default::default(),
            ledger: Default::default(),
            cache: Default::default(),
        }
    }
}


#[async_trait::async_trait(?Send)]
impl<T: RunLedger<L>, L: LedgerEvent> SpekiProvider<T, L> for PureMem<L>{
    async fn load_content(&self, space: &str, id: &str) -> Option<String>{todo!()}
    async fn load_all_contents(&self) -> HashMap<String, String>{todo!()}

    async fn save_content(&self, space: &str, id: String, record: String){
        self.state.write().unwrap().insert(id, record);
    }

    async fn save_cache(&self, key: String, ids: HashSet<String>) {
        self.cache.write().unwrap().insert(key, ids);
    }

    async fn load_cache(&self, key: &str) -> HashSet<String>{
        self.cache.read().unwrap().get(key).cloned().unwrap_or_default()
    }


    async fn current_time(&self) -> Duration{
        self.time.current_time()
    }
    /// Clear the storage area so we can re-run everything.
    async fn clear_space(&self, space: &str){todo!()}
    async fn clear_state(&self) {
        self.state.write().unwrap().clear();
    }
    async fn clear_ledger(&self) {
        self.ledger.write().unwrap().clear();
    }
    async fn load_ledger(&self) -> Vec<L>{
        let map = self.ledger.read().unwrap().clone();

        let mut foo: Vec<LedgerEntry<L>> = vec![];

        for (_, value) in map.iter(){
            foo.push(value.clone());
        }

        foo.sort_by_key(|k|k.timestamp);
        foo.into_iter().map(|e| e.event).collect()
    }

    async fn save_ledger(&self, event: LedgerEntry<L>){
        self.ledger.write().unwrap().insert(event.timestamp.as_micros() as u64, event);
    }
}


pub type Recaller = Arc<Box<dyn RecallCalc + Send>>;
pub type TimeGetter = Arc<Box<dyn TimeProvider + Send + Sync>>;

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
    pub fn new<A, B, C, D, E, F>(
        recall_calc: A,
        time_provider: B,
        card_provider: C,
        history_provider: D,
        collections_provider: E,
        meta_provider: F,
    ) -> Self
    where
        A: RecallCalc + 'static + Send,
        B: TimeProvider + 'static + Send + Sync,
        C: SpekiProvider<BaseCard, CardEvent> + 'static + Send,
        D: SpekiProvider<History, ReviewEvent> + 'static + Send,
        E: SpekiProvider<Collection, CollectionEvent> + 'static + Send,
        F: SpekiProvider<Metadata, MetaEvent> + 'static + Send,
    {
        info!("initialtize app");

        let time_provider: TimeGetter = Arc::new(Box::new(time_provider));
        let recaller: Recaller = Arc::new(Box::new(recall_calc));

        let provider = Provider {
            cards: Arc::new(Box::new(card_provider)),
            reviews: Arc::new(Box::new(history_provider)),
            collections: CollectionProvider {
                inner: Arc::new(Box::new(collections_provider)),
            },
            metadata: Arc::new(Box::new(meta_provider)),
            time: time_provider.clone(),
            
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

        let id = CardId::new_v4();

        let action = CardAction::UpsertCard { ty: data.into() };
        let event = CardEvent {
            action, id
        };

        self.provider.run_event(event).await;
        id
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
        let id = CardId::new_v4();
        let event = CardEvent::new(id, CardAction::UpsertCard { ty: data.into() });
        self.provider.run_event(event).await;
        id
    }

    pub async fn add_card_with_id(&self, front: String, back: impl Into<BackSide>, id: CardId) {
        let back = back.into();
        let data = NormalCard { front, back };
        let event = CardEvent::new(id, CardAction::UpsertCard { ty: data.into() });
        self.provider.run_event(event).await;
    }

    pub async fn add_card(&self, front: String, back: impl Into<BackSide>) -> CardId {
        let back = back.into();
        let data = NormalCard { front, back };

        let id = CardId::new_v4();
        let event = CardEvent::new(id, CardAction::UpsertCard { ty: data.into() });
        self.provider.run_event(event).await;
        id
    }

    pub async fn add_unfinished(&self, front: String) -> CardId {
        let data = UnfinishedCard { front };
        let id = CardId::new_v4();
        let event = CardEvent::new(id, CardAction::UpsertCard { ty: data.into() });
        self.provider.run_event(event).await;
        id
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


#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        //  App::new(recall_calc, time_provider, PureMem, history_provider, collections_provider, meta_provider)
        todo!()
    }

}