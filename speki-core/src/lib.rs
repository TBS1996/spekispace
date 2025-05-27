use attribute::AttrEvent;
use card::{Attrv2, BackSide, CardId, RawCard, RecallRate, TextData};
use card_provider::CardProvider;
use cardfilter::CardFilter;
use collection::{Collection, CollectionId};
use dioxus_logger::tracing::info;
use ledger::{CardAction, CardEvent, CollectionEvent, Event, MetaEvent};
use ledgerstore::CacheKey;
use ledgerstore::{Ledger, TimeProvider};
use metadata::Metadata;
use recall_rate::{History, ReviewEvent};
use set::{Set, SetEvent};
use std::fmt::Display;
use std::{collections::HashMap, fmt::Debug, sync::Arc, time::Duration};
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
pub mod set;

pub use attribute::{Attribute, AttributeId};
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
    ExplicitDependent,
    Instance,
    LinkRef,
    SubClass,
    AttrClass,
}

impl Display for RefType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_ref())
    }
}

impl AsRef<str> for RefType {
    fn as_ref(&self) -> &str {
        match self {
            Self::ExplicitDependent => "explicit_dependent",
            Self::Instance => "instances",
            Self::LinkRef => "linkref",
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
        CacheKey::ItemRef {
            reftype: value.ty.to_str().to_owned(),
            id: value.id.to_string(),
        }
    }
}

#[derive(Clone, PartialEq, PartialOrd, Hash, Eq, Debug)]
pub enum CardProperty {
    Bigram,
    Suspended,
    CardType,
    AttrId,
    /// mapping of attributeid -> CardId
    Attr,
}

impl Display for CardProperty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_ref())
    }
}

impl AsRef<str> for CardProperty {
    fn as_ref(&self) -> &str {
        match self {
            CardProperty::Bigram => "bigram",
            CardProperty::Suspended => "suspended",
            CardProperty::CardType => "cardtype",
            CardProperty::AttrId => "attr_id",
            CardProperty::Attr => "attr",
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
    pub inner: Ledger<Collection, CollectionEvent>,
}

impl CollectionProvider {
    pub fn new(inner: Ledger<Collection, CollectionEvent>) -> Self {
        Self { inner }
    }
    pub async fn save(&self, event: CollectionEvent) {
        todo!()
    }

    pub async fn load(&self, id: CollectionId) -> Option<Collection> {
        None
    }

    pub async fn load_all(&self) -> HashMap<CollectionId, Collection> {
        // self.inner.load_all_items().await.into_iter().map(|(key, val)| (key.parse().unwrap(), val)).collect()
        Default::default()
    }
}

#[derive(Clone)]
pub struct Provider {
    pub cards: Ledger<RawCard, CardEvent>,
    pub sets: Ledger<Set, SetEvent>,
    pub reviews: Ledger<History, ReviewEvent>,
    pub collections: Ledger<Collection, CollectionEvent>,
    pub metadata: Ledger<Metadata, MetaEvent>,
    pub time: TimeGetter,
}

impl Provider {
    pub fn run_event(&self, event: impl Into<Event>) {
        match event.into() {
            Event::Meta(event) => self.metadata.insert_ledger(event),
            Event::History(event) => self.reviews.insert_ledger(event),
            Event::Card(event) => self.cards.insert_ledger(event),
            Event::Collection(event) => self.collections.insert_ledger(event),
        }
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
    pub fn new<A, B>(
        recall_calc: A,
        time_provider: B,
        card_provider: Ledger<RawCard, CardEvent>,
        history_provider: Ledger<History, ReviewEvent>,
        collections_provider: Ledger<Collection, CollectionEvent>,
        meta_provider: Ledger<Metadata, MetaEvent>,
        set_provider: Ledger<Set, SetEvent>,
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
            collections: collections_provider,
            metadata: meta_provider,
            time: time_provider.clone(),
            sets: set_provider,
        };

        /*
        let mut map: HashMap<CardId, Vec<Attrv2>> = Default::default();
        for attribute in provider.attrs.load_all().into_values() {
            let attr = Attrv2 {
                id: attribute.id,
                pattern: attribute.pattern,
                back_type: attribute.back_type,
            };

            map.entry(attribute.class).or_default().push(attr);
        }

        for (card, attributes) in map {
            for attr in attributes {
                let action = CardAction::InsertAttr(attr);
                let event = CardEvent::new(card, action);
                provider.cards.insert_ledger(event);
            }
        }
        */

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

    pub fn load_card_sync(&self, id: CardId) -> Option<Card> {
        trace!("loading card: {id}");
        let card = self.card_provider.load(id);
        trace!("card loaded i guess: {card:?}");
        Some(Arc::unwrap_or_clone(card?))
    }

    pub async fn load_card(&self, id: CardId) -> Option<Card> {
        self.load_card_sync(id)
    }

    pub async fn load_cards(&self) -> Vec<CardId> {
        self.card_provider.load_all_card_ids()
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
            name: TextData::from_raw(&front),
            back,
            class,
        };
        let id = CardId::new_v4();
        let event = CardEvent::new(id, CardAction::UpsertCard(data));
        self.provider.run_event(event);
        id
    }

    pub async fn add_card_with_id(&self, front: String, back: impl Into<BackSide>, id: CardId) {
        let back = back.into();
        let data = CardType::Normal {
            front: TextData::from_raw(&front),
            back,
        };
        let event = CardEvent::new(id, CardAction::UpsertCard(data));
        self.provider.run_event(event);
    }

    pub async fn add_card(&self, front: String, back: impl Into<BackSide>) -> CardId {
        let back = back.into();
        let data = CardType::Normal {
            front: TextData::from_raw(&front),
            back,
        };

        let id = CardId::new_v4();
        let event = CardEvent::new(id, CardAction::UpsertCard(data));
        self.provider.run_event(event);
        id
    }

    pub async fn add_unfinished(&self, front: String) -> CardId {
        let data = CardType::Unfinished {
            front: TextData::from_raw(&front),
        };
        let id = CardId::new_v4();
        let event = CardEvent::new(id, CardAction::UpsertCard(data));
        self.provider.run_event(event);
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
            for child_id in card.dependencies() {
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
        format!("#{red:02X}{green:02X}00") // RGB color in hex
    }

    fn cyan_color() -> String {
        String::from("#00FFFF")
    }

    fn yellow_color() -> String {
        String::from("#FFFF00")
    }
}

/*

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use ledgerstore::{LedgerEvent, LedgerItem};
    use std::ops::Deref;
    use tracing::Level;
    use uuid::Uuid;

    use super::*;

    fn app() -> App {
        //  App::new(recall_calc, time_provider, PureMem, history_provider, collections_provider, meta_provider)
        todo!()
    }

    struct TestLedger<T: LedgerItem<E>, E: LedgerEvent> {
        pub ledger: Ledger<T, E>,
        root: PathBuf,
    }

    impl<T: LedgerItem<E>, E: LedgerEvent> TestLedger<T, E> {
        fn new() -> Self {
            let root =
                PathBuf::from("/home/tor/ledgertest").join(Uuid::new_v4().as_simple().to_string());
            Self {
                ledger: Ledger::new(root.as_path(), Arc::new(())),
                root,
            }
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

    fn new_card_with_id(front: &str, back: &str, id: CardId) -> CardEvent {
        let ty = CardType::Normal {
            front: TextData::from_raw(front),
            back: back.to_string().into(),
        };
        CardEvent::new(id, CardAction::UpsertCard(ty))
    }

    fn new_card(front: &str, back: &str) -> CardEvent {
        new_card_with_id(front, back, CardId::new_v4())
    }

    fn new_card_ledger() -> TestLedger<RawCard, CardEvent> {
        TestLedger::new()
    }

    /// hmm
    /// so i guess the api should be, you just insert a ledger, it doesnt do anything until you try to get an item
    /// itll then see theres unapplied entries, itll apply them and then use it
    #[tokio::test]
    async fn test_insert_retrieve() {
        let _ = tracing_subscriber::fmt()
            .with_max_level(Level::TRACE)
            .try_init()
            .ok();
        let ledger = new_card_ledger();
        std::thread::sleep(std::time::Duration::from_millis(200));
        let new_card = new_card("foo", "bar");
        let id = new_card.id;
        ledger.insert_ledger(new_card);

        info!("fom test loading ledger");
        std::thread::sleep(std::time::Duration::from_millis(200));
        let res = ledger.load(&id.to_string()).unwrap();
        info!("finished: {res:?}");
    }

    #[tokio::test]
    async fn test_prop_cache() {
        let _ = tracing_subscriber::fmt()
            .with_max_level(Level::TRACE)
            .try_init()
            .ok();
        let ledger = new_card_ledger();
        let card_a = new_card("hello", "bar");
        ledger.insert_ledger(card_a.clone());

        let bi: Vec<CardId> = ledger
            .get_prop_cache(CardProperty::Bigram, format!("el"))
            .into_iter()
            .map(|x| x.parse().unwrap())
            .collect();


        assert!(bi.contains(&card_a.id))
    }

    #[tokio::test]
    async fn test_ref_cache() {
        let _ = tracing_subscriber::fmt()
            .with_max_level(Level::TRACE)
            .try_init()
            .ok();
        let ledger = new_card_ledger();
        let card_a = new_card("a", "bar");
        let a_id = card_a.id;
        ledger.insert_ledger(card_a);

        let card_b = new_card("b", "foo");
        let b_id = card_b.id;
        ledger.insert_ledger(card_b);

        let card_c = new_card("c", "baz");
        let c_id = card_c.id;
        ledger.insert_ledger(card_c);

        let refaction = CardEvent::new(a_id, CardAction::AddDependency(c_id));
        ledger.insert_ledger(refaction);

        let refaction = CardEvent::new(b_id, CardAction::AddDependency(c_id));
        ledger.insert_ledger(refaction);

        let card_a = ledger.load(&a_id.to_string()).unwrap();
        let card_b = ledger.load(&b_id.to_string()).unwrap();




        let c_deps: Vec<CardId> = ledger
            .get_ref_cache(RefType::Dependent, c_id)
            .into_iter()
            .map(|x| x.parse().unwrap())
            .collect();

        assert!(c_deps.contains(&a_id));
        assert!(c_deps.contains(&b_id));
    }
}
*/
