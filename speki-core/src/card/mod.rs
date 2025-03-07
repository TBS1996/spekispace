use core::f32;
use std::{
    cmp::{Ord, Ordering, PartialEq},
    collections::{BTreeMap, BTreeSet},
    fmt::Debug,
    sync::Arc,
    time::Duration,
};

use futures::executor::block_on;
use serde::{ser::SerializeSeq, Deserializer};
use serde_json::Value;
use speki_dto::LedgerEvent;
use speki_dto::LedgerItem;
use tracing::info;
use uuid::Uuid;

use crate::{
    audio::{Audio, AudioId},
    card_provider::CardProvider,
    ledger::{CardAction, CardEvent, HistoryEvent, MetaEvent},
    metadata::{IsSuspended, Metadata},
    recall_rate::{History, Recall, Review, ReviewEvent, SimpleRecall},
    RecallCalc, Recaller, TimeGetter,
};

pub type RecallRate = f32;

mod basecard;

pub use basecard::*;

#[derive(Clone)]
pub struct Card {
    id: CardId,
    front_audio: Option<Audio>,
    back_audio: Option<Audio>,
    base: RawCard,
    metadata: Metadata,
    history: History,
    card_provider: CardProvider,
    recaller: Recaller,
}

impl PartialEq for Card {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Ord for Card {
    fn cmp(&self, other: &Self) -> Ordering {
        self.id.cmp(&other.id)
    }
}

impl PartialOrd for Card {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.id.partial_cmp(&other.id)
    }
}

impl Eq for Card {}

impl Debug for Card {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = String::new();
        s.push_str(&format!("{:?}\n", self.id));
        s.push_str(&format!("{:?}\n", self.base.data.ty));

        write!(f, "{}", s)
    }
}

impl std::fmt::Display for Card {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", block_on(self.print()))
    }
}

impl Card {
    pub fn front_audio(&self) -> Option<&Audio> {
        self.front_audio.as_ref()
    }

    pub fn back_audio(&self) -> Option<&Audio> {
        self.back_audio.as_ref()
    }

    pub fn front_audio_id(&self) -> Option<AudioId> {
        self.base.front_audio
    }

    pub fn back_audio_id(&self) -> Option<AudioId> {
        self.base.back_audio
    }

    pub async fn dependents(&self) -> BTreeSet<Arc<Self>> {
        let mut cards = BTreeSet::default();
        for card in self.card_provider.dependents(self.id).await {
            let card = self.card_provider.load(card).await.unwrap();
            cards.insert(card);
        }
        cards
    }

    /// Replaces current self with what's in card provider cache.
    async fn refresh(&mut self) {
        self.card_provider.invalidate_card(self.id);
        *self = Arc::unwrap_or_clone(self.card_provider.load(self.id).await.unwrap());
    }

    pub async fn add_review(&mut self, recall: Recall) {
        let event = ReviewEvent {
            id: self.id,
            grade: recall,
            timestamp: self.current_time(),
        };
        self.card_provider.providers.run_event(event).await;
    }

    pub fn time_provider(&self) -> TimeGetter {
        self.card_provider.time_provider()
    }

    pub fn lapses_last_month(&self) -> u32 {
        let current_time = self.time_provider().current_time();
        let day = Duration::from_secs(86400 * 30);

        self.history.lapses_since(day, current_time)
    }
    pub fn lapses_last_week(&self) -> u32 {
        let current_time = self.time_provider().current_time();
        let day = Duration::from_secs(86400 * 7);

        self.history.lapses_since(day, current_time)
    }

    pub fn lapses_last_day(&self) -> u32 {
        let current_time = self.time_provider().current_time();
        let day = Duration::from_secs(86400);

        self.history.lapses_since(day, current_time)
    }

    pub async fn from_parts(
        base: RawCard,
        history: History,
        metadata: Metadata,
        card_provider: CardProvider,
        recaller: Recaller,
        front_audio: Option<Audio>,
        back_audio: Option<Audio>,
    ) -> Self {
        let id = base.id;

        Self {
            id,
            base,
            metadata,
            history,
            card_provider,
            recaller,
            front_audio,
            back_audio,
        }
    }

    pub fn is_finished(&self) -> bool {
        self.base.data.ty != CType::Unfinished
    }

    pub fn is_class(&self) -> bool {
        self.base.data.ty == CType::Class
    }

    pub fn is_instance_of(&self, class: CardId) -> bool {
        if self.base.data.ty == CType::Instance {
            self.base.data.class().unwrap() == class
        } else {
            false
        }
    }

    pub fn is_instance(&self) -> bool {
        self.base.data.ty == CType::Instance
    }

    pub async fn set_ref(mut self, reff: CardId) -> Card {
        let backside = BackSide::Card(reff);
        self.base = self.base.set_backside(backside);
        let action = CardAction::SetBackRef(reff);
        let event = CardEvent::new(self.id, action);
        self.card_provider.providers.run_event(event).await;
        self.refresh().await;
        self
    }

    pub async fn rm_dependency(&mut self, dependency: CardId) {
        info!(
            "for removal, dependent: {}, -- dependency: {}",
            self.id(),
            dependency
        );
        let res = self.base.dependencies.remove(&dependency);

        if !res {
            info!("no dep to remove");
            return;
        }

        info!("dep was there: {res}");
        self.base.remove_dep(dependency);
        let action = CardAction::RemoveDependency(dependency);
        let event = CardEvent::new(self.id, action);
        self.card_provider.providers.run_event(event).await;
        self.refresh().await;
    }

    pub async fn add_dependency(&mut self, dependency: CardId) {
        self.base.dependencies.insert(dependency);
        let action = CardAction::AddDependency(dependency);
        let event = CardEvent::new(self.id, action);
        self.card_provider.providers.run_event(event).await;
        self.refresh().await;
    }

    pub fn back_side(&self) -> Option<BackSide> {
        self.base.data.backside()
    }

    pub async fn delete_card(self) {
        self.card_provider.remove_card(self.id).await;
    }

    pub async fn recursive_dependents(&self) -> Vec<CardId> {
        info!("getting dependents of: {}", self.id);
        let mut deps = vec![];
        let mut stack = vec![self.id()];

        while let Some(id) = stack.pop() {
            let card = self.card_provider.load(id).await.unwrap();

            if self.id() != id {
                deps.push(id);
            }

            for dep in card.dependents().await.into_iter().map(|card| card.id) {
                stack.push(dep);
            }
        }

        deps
    }

    pub async fn recursive_dependencies(&self) -> Vec<CardId> {
        tracing::trace!("getting dependencies of: {}", self.id);
        let mut deps = vec![];
        let mut stack = vec![self.id()];

        while let Some(id) = stack.pop() {
            let Some(card) = self.card_provider.load(id).await else {
                continue;
            };

            if self.id() != id {
                deps.push(id);
            }

            for dep in card.dependencies().await {
                stack.push(dep);
            }
        }

        deps
    }

    pub async fn min_rec_recall_rate(&self) -> RecallRate {
        tracing::trace!("min rec recall of {}", self.id);
        let mut min_recall: RecallRate = 1.0;

        for card in self.recursive_dependencies().await {
            let card = self.card_provider.load(card).await.unwrap();
            min_recall = min_recall.min(card.recall_rate().unwrap_or_default());
        }

        min_recall
    }

    pub async fn display_backside(&self) -> Option<String> {
        Some(match self.back_side()? {
            BackSide::Trivial => format!("…"),
            BackSide::Invalid => "invalid: referenced a deleted card".to_string(),
            BackSide::Time(time) => format!("🕒 {}", time),
            BackSide::Text(s) => s.to_owned(),
            BackSide::Card(id) => {
                format!(
                    "→ {}",
                    self.card_provider.load(id).await.unwrap().print().await
                )
            }
            BackSide::List(list) => format!("→ [{}]", {
                let mut res = vec![];
                for id in list {
                    let s = self.card_provider.load(id).await.unwrap().print().await;
                    res.push(s);
                }

                res.join(", ")
            }),
        })
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    fn current_time(&self) -> Duration {
        self.card_provider.time_provider().current_time()
    }

    fn time_passed_since_last_review(&self) -> Option<Duration> {
        self.history.time_since_last_review(self.current_time())
    }

    pub fn recall_rate_at(&self, current_unix: Duration) -> Option<RecallRate> {
        SimpleRecall.recall_rate(&self.history, current_unix)
    }

    pub fn recall_rate(&self) -> Option<RecallRate> {
        let now = self.current_time();
        self.recaller.recall_rate(&self.history, now)
    }

    pub fn maturity(&self) -> Option<f32> {
        use gkquad::single::integral;

        let now = self.current_time();
        let result = integral(
            |x: f64| {
                self.recall_rate_at(now + Duration::from_secs_f64(x * 86400.))
                    .unwrap_or_default() as f64
            },
            0.0..1000.,
        )
        .estimate()
        .ok()?;

        Some(result as f32)
    }

    pub async fn print(&self) -> String {
        self.base
            .data
            .front
            .clone()
            .unwrap_or_else(|| String::from("oops"))
    }

    pub fn is_pending(&self) -> bool {
        self.history.is_empty()
    }

    pub fn is_suspended(&self) -> bool {
        self.metadata.suspended.is_suspended()
    }

    pub async fn set_suspend(&mut self, suspend: bool) {
        let event = MetaEvent {
            id: self.id,
            action: crate::ledger::MetaAction::Suspend(suspend),
        };

        self.card_provider.providers.run_event(event).await;
        self.metadata.suspended = suspend.into();
        self.card_provider.invalidate_card(self.id);
        self.refresh().await;
    }

    pub fn time_since_last_review(&self) -> Option<Duration> {
        self.time_passed_since_last_review()
    }

    pub fn id(&self) -> CardId {
        self.id
    }

    pub async fn dependencies(&self) -> BTreeSet<CardId> {
        self.base.dependencies().await
    }

    pub fn lapses(&self) -> u32 {
        self.history.lapses()
    }
}

/*

#[cfg(test)]
mod tests {

    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
        time::Duration,
    };

    use async_trait::async_trait;
    use speki_dto::{BackSide, CType, Cty, Item, RawCard, RawType, Recall, Record, SpekiProvider};
    use uuid::Uuid;

    use super::Card;
    use crate::{
        card_provider::CardProvider, Provider, Recaller, SimpleRecall, TimeGetter, TimeProvider,
    };

    struct Storage {
        inner: Arc<Mutex<Inner>>,
        time: ControlledTime,
    }

    impl Storage {
        fn new(time: ControlledTime) -> Self {
            Self {
                time,
                inner: Arc::new(Mutex::new(Inner {
                    cards: Default::default(),
                    reviews: Default::default(),
                    _attrs: Default::default(),
                })),
            }
        }

        fn save(&self, ty: Cty, id: Uuid, s: String) {
            let mut lock = self.inner.lock().unwrap();
            let map = match ty {
                Cty::Attribute => &mut lock._attrs,
                Cty::Review => &mut lock.cards,
                Cty::Card => &mut lock.reviews,
            };

            let record = Record {
                id: id.to_string(),
                content: s,
                last_modified: self.time.current_time().as_secs(),
                inserted: None,
            };

            map.insert(id, record);
        }

        fn get_all(&self, ty: Cty) -> HashMap<Uuid, Record> {
            let lock = self.inner.lock().unwrap();

            match ty {
                Cty::Attribute => &lock._attrs,
                Cty::Review => &lock.cards,
                Cty::Card => &lock.reviews,
            }
            .clone()
        }

        fn get(&self, ty: Cty, id: Uuid) -> Option<Record> {
            let lock = self.inner.lock().unwrap();
            match ty {
                Cty::Attribute => lock._attrs.get(&id).cloned(),
                Cty::Card => lock.cards.get(&id).cloned(),
                Cty::Review => lock.reviews.get(&id).cloned(),
            }
        }
    }

    struct Inner {
        cards: HashMap<Uuid, Record>,
        reviews: HashMap<Uuid, Record>,
        _attrs: HashMap<Uuid, Record>,
    }

    #[async_trait(?Send)]
    impl<T: Item> SpekiProvider<T> for Storage {
        async fn current_time(&self) -> Duration {
            todo!()
        }

        async fn load_record(&self, id: Uuid) -> Option<Record> {
            self.get(T::identifier(), id)
        }

        async fn load_all_records(&self) -> HashMap<Uuid, Record> {
            self.get_all(T::identifier())
        }

        async fn save_record(&self, record: Record) {
            self.save(T::identifier(), record.id.parse().unwrap(), record.content);
        }
    }

    #[derive(Clone, Default)]
    struct ControlledTime {
        time: Arc<Mutex<Duration>>,
    }

    impl ControlledTime {
        fn inc(&self, inc: Duration) {
            *self.time.lock().unwrap() += inc;
        }
    }

    impl TimeProvider for ControlledTime {
        fn current_time(&self) -> Duration {
            *self.time.lock().unwrap()
        }
    }

    struct TestStuff {
        recaller: Recaller,
        card_provider: CardProvider,
        time_provider: ControlledTime,
    }

    impl TestStuff {
        fn new() -> Self {
            let timed = ControlledTime::default();
            let time_provider: TimeGetter = Arc::new(Box::new(timed.clone()));
            let recaller: Recaller = Arc::new(Box::new(SimpleRecall));
            let provider = Provider {
                cards: Arc::new(Box::new(Storage::new(timed.clone()))),
                reviews: Arc::new(Box::new(Storage::new(timed.clone()))),
                attrs: Arc::new(Box::new(Storage::new(timed.clone()))),
            };

            // let provider: Provider = Arc::new(Box::new(Storage::new(timed.clone())));
            let card_provider = CardProvider::new(provider, time_provider, recaller.clone());

            Self {
                recaller,
                card_provider,
                time_provider: timed,
            }
        }

        fn inc_time(&self, inc: Duration) {
            self.time_provider.inc(inc);
        }

        async fn insert_card(&self) -> Card {
            let raw = raw_dummy().await;
            let card = Card::from_raw(raw, self.card_provider.clone(), self.recaller.clone()).await;
            let id = card.id;
            self.card_provider.save_card(card).await;
            (*self.card_provider.load(id).await.unwrap()).clone()
        }
    }

    async fn raw_dummy() -> RawCard {
        let data = RawType {
            ty: CType::Normal,
            front: "front1".to_string().into(),
            back: Some(BackSide::Text("back1".to_string())),
            class: None,
            instance: None,
            attribute: None,
            start_time: None,
            end_time: None,
            parent_event: None,
        };

        let raw = RawCard {
            id: Uuid::new_v4(),
            data,
            dependencies: Default::default(),
            ..Default::default()
        };

        raw
    }

    #[tokio::test]
    async fn test_recall() {
        let storage = TestStuff::new();
        let mut card = storage.insert_card().await;
        assert!(card.recall_rate().is_none());

        card.add_review(Recall::Perfect).await;
        assert_eq!(card.recall_rate().unwrap(), 1.0);
        storage.inc_time(Duration::from_secs(100));
        assert!(card.recall_rate().unwrap() < 1.0);
    }

    #[tokio::test]
    async fn test_min_recall_basic() {
        let storage = TestStuff::new();
        let mut card1 = storage.insert_card().await;
        let mut card2 = storage.insert_card().await;

        card1.add_review(Recall::Late).await;
        card2.add_review(Recall::Some).await;

        storage.inc_time(Duration::from_secs(86400));

        card1.add_dependency(card2.id).await;

        let lowest_recall = card2.recall_rate().unwrap();

        assert_eq!(card1.min_rec_recall_rate().await, lowest_recall);
        assert_eq!(card2.min_rec_recall_rate().await, 1.0);
    }

    #[tokio::test]
    async fn test_min_recall_simple() {
        use tracing::Level;
        use tracing_subscriber::FmtSubscriber;

        let subscriber = FmtSubscriber::builder()
            .with_max_level(Level::INFO)
            .finish();

        // Set the global default subscriber.
        tracing::subscriber::set_global_default(subscriber)
            .expect("Setting default subscriber failed");

        let storage = TestStuff::new();
        let mut card1 = storage.insert_card().await;
        let mut card2 = storage.insert_card().await;
        let mut card3 = storage.insert_card().await;

        card1.add_review(Recall::Some).await;
        card2.add_review(Recall::Late).await;
        card3.add_review(Recall::Perfect).await;

        storage.inc_time(Duration::from_secs(86400));

        card1.add_dependency(card2.id).await;
        card2.add_dependency(card3.id).await;

        dbg!(card1.recall_rate().unwrap());
        dbg!(card2.recall_rate().unwrap());
        dbg!(card3.recall_rate().unwrap());

        tracing::info!("card1 min rec recall:");
        dbg!(card1.min_rec_recall_rate().await);
        tracing::info!("card2 min rec recall:");
        dbg!(card2.min_rec_recall_rate().await);
        tracing::info!("card3 min rec recall:");
        dbg!(card3.min_rec_recall_rate().await);

        assert_eq!(
            card1.min_rec_recall_rate().await,
            card2.recall_rate().unwrap()
        );
        assert_eq!(
            card2.min_rec_recall_rate().await,
            card3.recall_rate().unwrap()
        );
        assert_eq!(card3.min_rec_recall_rate().await, 1.0,);
    }

    #[tokio::test]
    async fn test_min_recall() {
        let storage = TestStuff::new();
        let mut card1 = storage.insert_card().await;
        let mut card2 = storage.insert_card().await;
        let mut card3 = storage.insert_card().await;
        let mut card4 = storage.insert_card().await;

        card1.add_review(Recall::Perfect).await;
        card2.add_review(Recall::Some).await;
        card3.add_review(Recall::None).await;
        card4.add_review(Recall::Late).await;

        storage.inc_time(Duration::from_secs(86400));

        card1.add_dependency(card2.id).await;
        card2.add_dependency(card3.id).await;
        card3.add_dependency(card4.id).await;

        let lowest_recall = card3.recall_rate().unwrap();

        assert_eq!(card1.min_rec_recall_rate().await, lowest_recall);
    }

    #[tokio::test]
    async fn test_recall_cache() {
        let storage = TestStuff::new();

        let mut card1 = storage.insert_card().await;
        let mut card2 = storage.insert_card().await;
        let mut card3 = storage.insert_card().await;
        let mut card4 = storage.insert_card().await;

        card1.add_review(Recall::None).await;
        card2.add_review(Recall::Some).await;
        card3.add_review(Recall::Late).await;
        card4.add_review(Recall::Perfect).await;

        storage.inc_time(Duration::from_secs(86400));

        tracing::info!("TEST ADD DEPENDENCIES");
        card1.add_dependency(card2.id).await;
        card2.add_dependency(card3.id).await;
        card3.add_dependency(card4.id).await;

        dbg!(card1.recall_rate().unwrap());
        dbg!(card2.recall_rate().unwrap());
        dbg!(card3.recall_rate().unwrap());
        dbg!(card4.recall_rate().unwrap());

        tracing::info!("card1 min rec recall:");
        dbg!(card1.min_rec_recall_rate().await);
        tracing::info!("card2 min rec recall:");
        dbg!(card2.min_rec_recall_rate().await);
        tracing::info!("card3 min rec recall:");
        dbg!(card3.min_rec_recall_rate().await);
        tracing::info!("card4 min rec recall:");
        dbg!(card4.min_rec_recall_rate().await);

        assert_eq!(
            card1.min_rec_recall_rate().await,
            card3.recall_rate().unwrap()
        );
        assert_eq!(
            card2.min_rec_recall_rate().await,
            card3.recall_rate().unwrap()
        );
        assert_eq!(
            card3.min_rec_recall_rate().await,
            card4.recall_rate().unwrap()
        );
        assert_eq!(card4.min_rec_recall_rate().await, 1.0);
    }
}

*/
