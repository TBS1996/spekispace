use std::{collections::{HashMap, HashSet}, fmt::Debug, hash, sync::{Arc, RwLock}, time::Duration};
use card::{BackSide, BaseCard, CType, CardId, RecallRate};
use card_provider::CardProvider;
use cardfilter::{CardFilter};
use collection::{Collection, CollectionId};
use dioxus_logger::tracing::info;
use eyre::Result;
use ledger::{check_compose, decompose, decompose_history, CardAction, CardEvent, CollectionEvent, Event, HistoryEvent, MetaEvent};
use metadata::Metadata;
use recall_rate::{History, ReviewEvent};
use serde::{de::DeserializeOwned, Serialize};
use speki_dto::{Ledger, LedgerEntry, LedgerEvent, RunLedger, Storage, TimeProvider};
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
   inner: Ledger<Collection, CollectionEvent>,
}

impl CollectionProvider {
    pub fn new(inner: Ledger<Collection, CollectionEvent>) -> Self {
        Self {
            inner
        }
    }
    pub async fn save(&self, event: CollectionEvent) {
        self.inner.save_and_run(event).await;
    }

    pub async fn load(&self, id: CollectionId) -> Option<Collection> {
        return None;
    }

    pub async fn load_all(&self) -> HashMap<CollectionId, Collection> {
       // self.inner.load_all_items().await.into_iter().map(|(key, val)| (key.parse().unwrap(), val)).collect()
        Default::default()
    }
}

#[derive(Clone)]
pub struct Provider {
    pub cards: Ledger<BaseCard, CardEvent>,
    pub reviews: Ledger<History, ReviewEvent>,
    pub collections: CollectionProvider,
    pub metadata: Ledger<Metadata, MetaEvent>,
    pub time: TimeGetter,
}

impl Provider {
    async fn run_card_event(&self, event: CardEvent) {
        self.cards.save_and_run(event).await;
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

        todo!();
    }

    pub async fn run_event(&self, event: impl Into<Event>) {
        match event.into() {
            Event::Meta(event) => self.run_meta_event(event).await,
            Event::History(event) => self.run_history_event(event).await,
            Event::Card(event) => self.run_card_event(event).await,
            Event::Collection(col) => self.collections.inner.save_and_run(col).await,
        }
    }

    async fn run_history_event(&self, event: ReviewEvent) {
        self.reviews.save_and_run(event).await;
    }

    async fn run_meta_event(&self, event: MetaEvent) {
        self.metadata.save_and_run(event).await;
    }
}


#[derive(Clone)]
pub struct MemStorage{
    storage: Arc<RwLock<HashMap<String, HashMap<String, String>>>>,
}

#[async_trait::async_trait(?Send)]
impl<T: Serialize + DeserializeOwned + 'static> Storage<T> for MemStorage{
    async fn load_content(&self, space: &str, id: &str) -> Option<String> {
        self.storage.read().unwrap().get(space)?.get(id).cloned()
    }
    async fn load_all_contents(&self, space: &str) -> HashMap<String, String> {
        self.storage.read().unwrap().get(space).cloned().unwrap_or_default()
    }
    async fn save_content(&self, space: &str, id: &str, record: String) {
        self.storage.write().unwrap().entry(space.to_string()).or_default().insert(id.to_string(), record);
    }

    async fn clear_space(&self, space: &str) {
        self.storage.write().unwrap().remove(space);
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


pub async fn import_card_ledger(ledger: Ledger<BaseCard, CardEvent>) {
    let path = std::path::PathBuf::from("/home/tor/Downloads/spekicards.json");

    #[derive(Debug, serde::Deserialize)]
    struct Lol {
        records: Vec<serde_json::Value>
    }

    let s = std::fs::read_to_string(&path).unwrap();

    let y: Lol = serde_json::from_str(&s).unwrap();

    for rec in y.records {
        let val = rec.get("content").unwrap().as_str().unwrap().to_string();
        let card:  BaseCard = toml::from_str(&val).unwrap();
        let actions = decompose(&card);
        for action in actions {
            ledger.save_and_run(action).await;
        }
    }
}


pub async fn import_history_ledger(ledger: Ledger<History, ReviewEvent>) {
    let path = std::path::PathBuf::from("/home/tor/Downloads/spekireviews.json");

    #[derive(Debug, serde::Deserialize)]
    struct Lol {
        records: Vec<serde_json::Value>
    }

    let s = std::fs::read_to_string(&path).unwrap();

    let y: Lol = serde_json::from_str(&s).unwrap();

    for rec in y.records {
        let val = rec.get("content").unwrap().as_str().unwrap().to_string();
        let card:  History = toml::from_str(&val).unwrap();
        let actions = decompose_history(card);
        for action in actions {
            ledger.save_and_run(action).await;
        }
    }
}

impl App {
    pub fn new<A, B>(
        recall_calc: A,
        time_provider: B,
        card_provider: Ledger<BaseCard, CardEvent>,
        history_provider: Ledger<History, ReviewEvent>,
        collections_provider: Ledger<Collection, CollectionEvent>,
        meta_provider: Ledger<Metadata, MetaEvent>,
    ) -> Self
    where
        A: RecallCalc + 'static + Send,
        B: TimeProvider + 'static + Send + Sync,
    {
        info!("initialtize app");

        let time_provider: TimeGetter = Arc::new(Box::new(time_provider));
        let recaller: Recaller = Arc::new(Box::new(recall_calc));

        let provider = Provider {
            cards: card_provider,
            reviews: history_provider,
            collections: CollectionProvider {
                inner: collections_provider,
            },
            metadata:meta_provider,
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
        //bruh(self.provider.reviews.clone()).await;
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

        let action = CardAction::UpsertCard ( data.into() );
        let event = CardEvent::new(id, action);

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
        let event = CardEvent::new(id, CardAction::UpsertCard ( data.into() ));
        self.provider.run_event(event).await;
        id
    }

    pub async fn add_card_with_id(&self, front: String, back: impl Into<BackSide>, id: CardId) {
        let back = back.into();
        let data = NormalCard { front, back };
        let event = CardEvent::new(id, CardAction::UpsertCard ( data.into() ));
        self.provider.run_event(event).await;
    }

    pub async fn add_card(&self, front: String, back: impl Into<BackSide>) -> CardId {
        let back = back.into();
        let data = NormalCard { front, back };

        let id = CardId::new_v4();
        let event = CardEvent::new(id, CardAction::UpsertCard ( data.into() ));
        self.provider.run_event(event).await;
        id
    }

    pub async fn add_unfinished(&self, front: String) -> CardId {
        let data = UnfinishedCard { front };
        let id = CardId::new_v4();
        let event = CardEvent::new(id, CardAction::UpsertCard ( data.into() ));
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