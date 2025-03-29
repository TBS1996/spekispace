use card::{BackSide, CType, CardId, RawCard, RecallRate};
use card_provider::CardProvider;
use cardfilter::CardFilter;
use collection::{Collection, CollectionId};
use dioxus_logger::tracing::info;
use ledger::{decompose_history, CardAction, CardEvent, CollectionEvent, Event, MetaEvent};
use metadata::Metadata;
use recall_rate::{History, ReviewEvent};
use snapstore::{CacheKey, PropertyCacheKey, RefCacheKey};
use speki_dto::{Ledger, LedgerEntry, Storage, TimeProvider};
use std::{
    collections::HashMap,
    fmt::Debug,
    sync::{Arc, RwLock},
    time::Duration,
};
use tracing::trace;

pub mod attribute;
pub mod audio;
pub mod card;
pub mod card_provider;
pub mod cardfilter;
pub mod collection;
mod common;
pub mod ledger;
pub mod metadata;
pub mod recall_rate;

pub use attribute::{Attribute, AttributeDTO, AttributeId};
pub use card::{Card, CardType};
pub use common::current_time;
pub use omtrent::TimeStamp;
pub use recall_rate::SimpleRecall;

#[derive(Clone, PartialEq, PartialOrd, Hash, Eq, Debug)]
pub struct DepCacheKey {
    id: CardId,
    ty: RefType,
}

#[derive(Clone, PartialEq, PartialOrd, Hash, Eq, Debug)]
pub enum RefType {
    Dependent,
    Instance,
    BackRef,
    SubClass,
    AttrClass,
}

impl AsRef<str> for RefType {
    fn as_ref(&self) -> &str {
        match self {
            Self::Dependent => "dependents",
            Self::Instance => "instances",
            Self::BackRef => "backrefs",
            Self::SubClass => "subclasses",
            Self::AttrClass => "attrclass",
        }
    }
}

impl RefType {
    pub fn to_str(&self) -> &str {
        self.as_ref()
    }
}

impl From<DepCacheKey> for CacheKey {
    fn from(value: DepCacheKey) -> Self {
        CacheKey::ItemRef(RefCacheKey { reftype: value.ty.to_str().to_owned(), id: value.id.to_string() })

    }
}

#[derive(Clone, PartialEq, PartialOrd, Hash, Eq, Debug)]
pub enum CardProperty {
    Bigram,
    Suspended,
    CardType,
    AttrId,
}

impl AsRef<str> for CardProperty {
    fn as_ref(&self) -> &str {
        match self {
            CardProperty::Bigram => "bigram",
            CardProperty::Suspended => "suspended",
            CardProperty::CardType => "cardtype",
            CardProperty::AttrId => "attr_id",
        }
    }
}

/*
impl CardProperty {
    /// Gets identifier and value of cache entry
    pub fn to_parts(&self) -> (&'static str, String) {
        let proptype: &'static str = self.as_ref();

        match self {
            CardProperty::Bigram([a, b]) => (proptype, format!("{a}{b}")),
            CardProperty::Suspended(flag) => (proptype, format!("{flag}")),
            CardProperty::CardType(ctype) => (proptype, format!("{ctype:?}")),
            CardProperty::AttrId(id) => (proptype, id.to_string()),
        }
    }
}
0*/

pub trait RecallCalc {
    fn recall_rate(&self, reviews: &History, current_unix: Duration) -> Option<RecallRate>;
}

#[derive(Clone)]
pub struct CollectionProvider {
    inner: Ledger<Collection, CollectionEvent>,
}

impl CollectionProvider {
    pub fn new(inner: Ledger<Collection, CollectionEvent>) -> Self {
        Self { inner }
    }
    pub async fn save(&self, event: CollectionEvent) {
        todo!()
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
    pub cards: Ledger<RawCard, CardEvent>,
    pub reviews: Ledger<History, ReviewEvent>,
    pub collections: CollectionProvider,
    pub metadata: Ledger<Metadata, MetaEvent>,
    pub time: TimeGetter,
}

impl Provider {
    async fn run_card_event(&self, event: CardEvent) {
        self.cards.save_ledger(event).await;
    }

    pub async fn run_event(&self, event: impl Into<Event>) {
        match event.into() {
            Event::Meta(event) => self.run_meta_event(event).await,
            Event::History(event) => self.run_history_event(event).await,
            Event::Card(event) => self.run_card_event(event).await,
            Event::Collection(col) => todo!(),
        }
    }

    async fn run_history_event(&self, event: ReviewEvent) {
        todo!()
    }

    async fn run_meta_event(&self, event: MetaEvent) {
        todo!()
    }
}

#[derive(Clone)]
pub struct MemStorage {
    storage: Arc<RwLock<HashMap<String, HashMap<String, Vec<u8>>>>>,
}

#[async_trait::async_trait(?Send)]
impl Storage for MemStorage {
    async fn load_content(&self, space: &[&str], id: &str) -> Option<Vec<u8>> {
        let space = space.join("::");
        self.storage.read().unwrap().get(&space)?.get(id).cloned()
    }
    async fn load_all_contents(&self, space: &[&str]) -> HashMap<String, Vec<u8>> {
        let space = space.join("::");
        self.storage
            .read()
            .unwrap()
            .get(&space)
            .cloned()
            .unwrap_or_default()
    }
    async fn save_content(&self, space: &[&str], id: &str, content: &[u8]) {
        let space = space.join("::");
        self.storage
            .write()
            .unwrap()
            .entry(space.to_string())
            .or_default()
            .insert(id.to_string(), content.to_owned());
    }

    async fn clear_space(&self, space: &[&str]) {
        let space = space.join("::");
        self.storage.write().unwrap().remove(&space);
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

pub async fn import_card_ledger(ledger: Ledger<RawCard, CardEvent>) {
    let path = std::path::PathBuf::from("/home/tor/Downloads/spekicards.json");

    #[derive(Debug, serde::Deserialize)]
    struct Lol {
        records: Vec<serde_json::Value>,
    }

    let s = std::fs::read_to_string(&path).unwrap();

    let y: Lol = serde_json::from_str(&s).unwrap();

    let prev: Option<LedgerEntry<CardEvent>> = None;

    for rec in y.records {
        let val = rec.get("content").unwrap().as_str().unwrap().to_string();
        let card: RawCard = toml::from_str(&val).unwrap();
        todo!()
        //let actions = decompose(&card);
        //for action in actions {
        //    prev = Some(ledger.xsave_ledger(action, prev).await);
        //}
    }
}

pub async fn import_history_ledger(ledger: Ledger<History, ReviewEvent>) {
    todo!();
    let path = std::path::PathBuf::from("/home/tor/Downloads/spekireviews.json");

    #[derive(Debug, serde::Deserialize)]
    struct Lol {
        records: Vec<serde_json::Value>,
    }

    let s = std::fs::read_to_string(&path).unwrap();

    let y: Lol = serde_json::from_str(&s).unwrap();
    let mut prev: Option<LedgerEntry<ReviewEvent>> = None;

    for rec in y.records {
        let val = rec.get("content").unwrap().as_str().unwrap().to_string();
        let card: History = toml::from_str(&val).unwrap();
        let actions = decompose_history(card);
        for action in actions {
            //prev = Some(ledger.xsave_ledger(action, prev).await);
        }
    }
}

impl App {
    pub fn new<A, B>(
        recall_calc: A,
        time_provider: B,
        card_provider: Ledger<RawCard, CardEvent>,
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
            metadata: meta_provider,
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

    pub async fn add_instance(
        &self,
        front: String,
        back: Option<impl Into<BackSide>>,
        class: CardId,
    ) -> CardId {
        let back = back.map(|back| back.into());
        let data = CardType::Instance {
            name: front,
            back,
            class,
        };
        let id = CardId::new_v4();
        let event = CardEvent::new(id, CardAction::UpsertCard(data));
        self.provider.run_event(event).await;
        id
    }

    pub async fn add_card_with_id(&self, front: String, back: impl Into<BackSide>, id: CardId) {
        let back = back.into();
        let data = CardType::Normal { front, back };
        let event = CardEvent::new(id, CardAction::UpsertCard(data.into()));
        self.provider.run_event(event).await;
    }

    pub async fn add_card(&self, front: String, back: impl Into<BackSide>) -> CardId {
        let back = back.into();
        let data = CardType::Normal { front, back };

        let id = CardId::new_v4();
        let event = CardEvent::new(id, CardAction::UpsertCard(data.into()));
        self.provider.run_event(event).await;
        id
    }

    pub async fn add_unfinished(&self, front: String) -> CardId {
        let data = CardType::Unfinished { front };
        let id = CardId::new_v4();
        let event = CardEvent::new(id, CardAction::UpsertCard(data.into()));
        self.provider.run_event(event).await;
        id
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
    use std::{fs, path::{Path, PathBuf}};

    use snapstore::{LedgerEvent, LedgerItem};
    use tracing::Level;
    use uuid::Uuid;
    use std::ops::Deref;

    use super::*;

    fn app() -> App {
        //  App::new(recall_calc, time_provider, PureMem, history_provider, collections_provider, meta_provider)
        todo!()
    }

    struct TestLedger<T: LedgerItem<E>, E: LedgerEvent>{ledger: Ledger<T, E>, root: PathBuf}

    impl<T: LedgerItem<E>, E: LedgerEvent> TestLedger<T, E> {
        fn new() -> Self {
            let root = PathBuf::from("/home/tor/ledgertest").join(Uuid::new_v4().as_simple().to_string());
            Self{ledger: Ledger::new(root.as_path()), root}
        }
    }

    impl<T: LedgerItem<E>, E: LedgerEvent> Deref for TestLedger<T, E> {
        type Target = Ledger<T, E>;

        fn deref(&self) -> &Self::Target {
            &self.ledger
        }
    }

    impl<T: LedgerItem<E>, E: LedgerEvent> Drop for TestLedger<T, E> {
        fn drop(&mut self) {
            //fs::remove_dir_all(&self.root).unwrap()
        }
    }

    fn new_card(front: &str, back: &str) -> CardEvent {
        let ty = CardType::Normal { front: front.to_string(), back: back.to_string().into() };
        CardEvent::new(Uuid::new_v4(), CardAction::UpsertCard(ty))
    }




    fn new_card_ledger() -> TestLedger<RawCard, CardEvent> {
        TestLedger::new()
    }

    /// hmm 
    /// so i guess the api should be, you just insert a ledger, it doesnt do anything until you try to get an item
    /// itll then see theres unapplied entries, itll apply them and then use it
    #[tokio::test]
    async fn test_insert_retrieve(){
        let _ = tracing_subscriber::fmt().with_max_level(Level::TRACE).try_init().unwrap();
        let ledger = new_card_ledger();
        let new_card = new_card("foo", "bar");
        let id = new_card.id;
        ledger.insert_ledger(new_card).await;

        info!("fom test loading ledger");
        let res = ledger.load(&id.to_string()).await.unwrap();
        info!("finished: {res:?}");
    }

    #[tokio::test]
    async fn test_ref_cache() {
        let _ = tracing_subscriber::fmt().with_max_level(Level::TRACE).try_init().ok();
        let ledger = new_card_ledger();
        let card_a = new_card("a", "bar");
        let a_id = card_a.id;
        ledger.insert_ledger(card_a).await;


        let card_b = new_card("b", "foo");
        let b_id = card_b.id;
        ledger.insert_ledger(card_b).await;

        let refaction = CardEvent::new(a_id, CardAction::AddDependency(b_id));
        ledger.insert_ledger(refaction).await;

        let card_a = ledger.load(&a_id.to_string()).await.unwrap();

        assert!(card_a.dependencies.contains(&b_id));
    }


}
