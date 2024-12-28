use std::cmp::Ord;
use std::cmp::PartialEq;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use core::f32;
use dioxus_logger::tracing::instrument;
use samsvar::json;
use samsvar::Matcher;
use serializing::from_any;
use serializing::into_any;
use speki_dto::BackSide;
use speki_dto::CType;
use speki_dto::CardId;
use speki_dto::RawCard;
use speki_dto::Recall;
use speki_dto::Review;
use tracing::info;

use crate::card_provider::CardProvider;
use crate::recall_rate::SimpleRecall;
use crate::reviews::Reviews;
use crate::RecallCalc;
use crate::Recaller;
use crate::TimeGetter;

pub type RecallRate = f32;

mod card_types;
pub(crate) mod serializing;

pub use card_types::*;
pub use serializing::new_raw_card;

#[async_trait::async_trait(?Send)]
pub trait CardTrait: Debug + Clone {
    async fn get_dependencies(&self) -> BTreeSet<CardId>;
    async fn display_front(&self) -> String;
}

#[derive(Ord, PartialOrd, Eq, PartialEq, Hash, Debug, Clone)]
pub enum IsSuspended {
    False,
    True,
    // Card is temporarily suspended, until contained unix time has passed.
    TrueUntil(Duration),
}

impl From<bool> for IsSuspended {
    fn from(value: bool) -> Self {
        match value {
            true => Self::True,
            false => Self::False,
        }
    }
}

impl Default for IsSuspended {
    fn default() -> Self {
        Self::False
    }
}

impl IsSuspended {
    fn verify_time(self, current_time: Duration) -> Self {
        if let Self::TrueUntil(dur) = self {
            if dur < current_time {
                return Self::False;
            }
        }
        self
    }

    pub fn is_suspended(&self) -> bool {
        !matches!(self, IsSuspended::False)
    }
}

#[derive(Debug, Clone)]
pub enum AnyType {
    Instance(InstanceCard),
    Normal(NormalCard),
    Unfinished(UnfinishedCard),
    Attribute(AttributeCard),
    Class(ClassCard),
    Statement(StatementCard),
    Event(EventCard),
}

impl AnyType {
    fn invalidate_backside(&mut self, id: CardId) {
        let backside = match self {
            AnyType::Instance(InstanceCard { back, .. }) => back.as_mut(),
            AnyType::Normal(NormalCard { back, .. }) => Some(back),
            AnyType::Unfinished(_) => None,
            AnyType::Attribute(AttributeCard { back, .. }) => Some(back),
            AnyType::Class(ClassCard { back, .. }) => Some(back),
            AnyType::Statement(_) => None,
            AnyType::Event(_) => None,
        };

        if let Some(back) = backside {
            back.invalidate_if_has_ref(id);
        }
    }

    // if a card is deleted that is being referenced we might have to change the card type
    pub fn remove_dep(&mut self, id: CardId) {
        self.invalidate_backside(id);

        match self {
            AnyType::Instance(InstanceCard {
                ref name,
                ref back,
                class,
            }) => {
                if *class == id {
                    match back.clone() {
                        Some(backside) => {
                            *self = Self::Normal(NormalCard {
                                front: name.clone(),
                                back: backside,
                            })
                        }
                        None => {
                            *self = Self::Unfinished(UnfinishedCard {
                                front: name.clone(),
                            })
                        }
                    }
                }
            }
            AnyType::Normal(_) => {}
            AnyType::Unfinished(_) => {}
            AnyType::Attribute(_) => {}
            AnyType::Class(ClassCard {
                name,
                back,
                parent_class,
            }) => {
                if *parent_class == Some(id) {
                    *self = Self::Class(ClassCard {
                        name: name.clone(),
                        back: back.clone(),
                        parent_class: None,
                    });
                }
            }
            AnyType::Statement(_) => {}
            AnyType::Event(_) => {}
        };
    }

    pub fn type_name(&self) -> &str {
        match self {
            AnyType::Unfinished(_) => "unfinished",
            AnyType::Statement(_) => "statement",
            AnyType::Attribute(_) => "attribute",
            AnyType::Instance(_) => "instance",
            AnyType::Normal(_) => "normal",
            AnyType::Class(_) => "class",
            AnyType::Event(_) => "event",
        }
    }

    /// This is mainly just so i dont forget to update the CType when the AnyType changes
    pub fn fieldless(&self) -> CType {
        match self {
            AnyType::Instance(_) => CType::Instance,
            AnyType::Normal(_) => CType::Normal,
            AnyType::Unfinished(_) => CType::Unfinished,
            AnyType::Attribute(_) => CType::Attribute,
            AnyType::Class(_) => CType::Class,
            AnyType::Statement(_) => CType::Statement,
            AnyType::Event(_) => CType::Event,
        }
    }

    pub fn is_class(&self) -> bool {
        matches!(self, Self::Class(_))
    }
    pub fn is_instance(&self) -> bool {
        matches!(self, Self::Instance(_))
    }
    pub fn is_finished(&self) -> bool {
        !matches!(self, Self::Unfinished(_))
    }

    pub fn set_backside(self, new_back: BackSide, card_provider: &CardProvider) -> Self {
        match self {
            x @ AnyType::Event(_) => x,
            x @ AnyType::Instance(_) => x,
            x @ AnyType::Statement(_) => x,
            AnyType::Normal(NormalCard { front, .. }) => NormalCard {
                front,
                back: new_back,
            }
            .into(),
            AnyType::Unfinished(UnfinishedCard { front }) => NormalCard {
                front,
                back: new_back,
            }
            .into(),
            AnyType::Attribute(AttributeCard {
                attribute,
                instance: concept_card,
                ..
            }) => AttributeCard {
                attribute,
                back: new_back,
                instance: concept_card,
                card_provider: card_provider.clone(),
            }
            .into(),
            Self::Class(class) => ClassCard {
                name: class.name,
                back: new_back,
                parent_class: class.parent_class,
            }
            .into(),
        }
    }
}

#[async_trait::async_trait(?Send)]
impl CardTrait for AnyType {
    async fn get_dependencies(&self) -> BTreeSet<CardId> {
        match self {
            AnyType::Instance(card) => card.get_dependencies().await,
            AnyType::Normal(card) => card.get_dependencies().await,
            AnyType::Unfinished(card) => card.get_dependencies().await,
            AnyType::Attribute(card) => card.get_dependencies().await,
            AnyType::Class(card) => card.get_dependencies().await,
            AnyType::Statement(card) => card.get_dependencies().await,
            AnyType::Event(card) => card.get_dependencies().await,
        }
    }

    async fn display_front(&self) -> String {
        match self {
            AnyType::Instance(card) => card.display_front().await,
            AnyType::Normal(card) => card.display_front().await,
            AnyType::Unfinished(card) => card.display_front().await,
            AnyType::Attribute(card) => card.display_front().await,
            AnyType::Class(card) => card.display_front().await,
            AnyType::Statement(card) => card.display_front().await,
            AnyType::Event(card) => card.display_front().await,
        }
    }
}

/// Represents a card that has been saved as a toml file, which is basically anywhere in the codebase
/// except for when youre constructing a new card.
/// Don't save this in containers or pass to functions, rather use the Id, and get new instances of SavedCard from the cache.
/// Also, every time you mutate it, call the persist() method.
#[derive(Clone)]
pub struct Card<T: CardTrait + ?Sized> {
    pub id: CardId,
    pub data: T,
    pub dependencies: BTreeSet<CardId>,
    pub tags: BTreeMap<String, String>,
    pub history: Reviews,
    pub suspended: IsSuspended,
    pub card_provider: CardProvider,
    pub recaller: Recaller,
    pub last_modified: Duration,
}

impl<T: CardTrait + ?Sized> PartialEq for Card<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

use std::cmp::Ordering;

impl<T: CardTrait + ?Sized> Ord for Card<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.id.cmp(&other.id)
    }
}

impl<T: CardTrait + ?Sized> PartialOrd for Card<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.id.partial_cmp(&other.id)
    }
}

impl<T: CardTrait + ?Sized> Eq for Card<T> {}

impl<T: CardTrait + ?Sized> Debug for Card<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = String::new();
        s.push_str(&format!("{:?}\n", self.id));
        s.push_str(&format!("{:?}\n", self.data));

        write!(f, "{}", s)
    }
}

use futures::executor::block_on;

impl<T: CardTrait> std::fmt::Display for Card<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", block_on(self.data.display_front()))
    }
}

impl<T: CardTrait> AsRef<CardId> for Card<T> {
    fn as_ref(&self) -> &CardId {
        &self.id
    }
}

impl Card<AnyType> {
    pub fn get_ty(&self) -> AnyType {
        self.data.clone()
    }

    /// Loads all the ancestor ancestor classes
    /// for example, king, human male, human
    pub async fn load_ancestor_classes(&self) -> Vec<CardId> {
        let mut classes = vec![];
        let mut parent_class = self.parent_class();

        while let Some(class) = parent_class {
            classes.push(class);
            parent_class = self.card_provider.load(class).await.unwrap().parent_class();
        }

        classes
    }

    pub async fn dependents(&self) -> BTreeSet<Arc<Self>> {
        self.card_provider.dependents(self.id).await
    }

    pub fn to_raw(&self) -> RawCard {
        self.clone().into()
    }

    pub async fn update_with_raw(&mut self, raw: RawCard) {
        assert_eq!(self.id.into_inner(), raw.id);
        *self = Self::from_raw(raw, self.card_provider.clone(), self.recaller.clone()).await;
        self.persist().await;
    }

    pub async fn add_review(&mut self, recall: Recall) {
        let review = Review {
            timestamp: self.time_provider().current_time(),
            grade: recall,
            time_spent: Default::default(),
        };

        self.history.0.push(review);
        self.persist().await;
    }

    pub fn override_reviews(&mut self, reviews: Vec<Review>) {
        self.history = Reviews(reviews);
    }

    pub async fn should_review(&self) -> bool {
        if self.recall_rate().unwrap_or_default() > 0.8 {
            return false;
        }

        /*
        if self.min_rec_recall_rate().await < 0.8 {
            return false;
        }
        */

        if self.is_suspended() {
            return false;
        }

        if self.lapses_last_day() > 2 {
            return false;
        }

        if self.lapses_last_week() > 5 {
            return false;
        }

        if self.lapses_last_month() > 7 {
            return false;
        }

        true
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

    pub fn from_raw_with_reviews(
        raw_card: RawCard,
        card_provider: CardProvider,
        recaller: Recaller,
        reviews: Vec<Review>,
    ) -> Card<AnyType> {
        let id = CardId(raw_card.id);

        Card::<AnyType> {
            id,
            data: into_any(raw_card.data, &card_provider),
            dependencies: raw_card.dependencies.into_iter().map(CardId).collect(),
            tags: raw_card.tags,
            history: Reviews(reviews),
            suspended: IsSuspended::from(raw_card.suspended),
            card_provider,
            recaller,
            last_modified: Duration::default(),
        }
    }

    pub async fn from_raw(
        raw_card: RawCard,
        card_provider: CardProvider,
        recaller: Recaller,
    ) -> Card<AnyType> {
        let id = CardId(raw_card.id);
        let reviews = card_provider.load_reviews(id).await;

        Self::from_raw_with_reviews(raw_card, card_provider, recaller, reviews.0)
    }

    pub fn card_type(&self) -> &AnyType {
        &self.data
    }

    /// Returns the class this card belongs to (if any)
    pub fn parent_class(&self) -> Option<CardId> {
        match &self.data {
            AnyType::Instance(instance) => Some(instance.class),
            AnyType::Class(class) => class.parent_class,
            AnyType::Normal(_) => None,
            AnyType::Unfinished(_) => None,
            AnyType::Attribute(_) => None,
            AnyType::Statement(_) => None,
            AnyType::Event(_) => None,
        }
    }

    pub fn is_finished(&self) -> bool {
        self.data.is_finished()
    }

    pub fn is_class(&self) -> bool {
        self.data.is_class()
    }

    pub fn is_instance(&self) -> bool {
        self.data.is_instance()
    }

    pub async fn set_ref(mut self, reff: CardId) -> Card<AnyType> {
        let backside = BackSide::Card(reff);
        self.data = self.data.set_backside(backside, &self.card_provider);
        self.persist().await;
        self
    }

    pub async fn rm_dependency(&mut self, dependency: CardId) -> bool {
        let res = self.dependencies.remove(&dependency);
        self.data.remove_dep(dependency);
        self.persist().await;
        res
    }

    pub async fn refresh(self) -> Arc<Self> {
        info!("refreshing card: {}", self.id);
        self.card_provider.load(self.id).await.unwrap()
    }

    pub async fn add_dependency(&mut self, dependency: CardId) {
        info!("for card: {} inserting dependency: {}", self.id, dependency);
        if self.id() == dependency {
            return;
        }
        self.dependencies.insert(dependency);
        self.persist().await;
    }

    pub fn back_side(&self) -> Option<&BackSide> {
        match self.card_type() {
            AnyType::Instance(instance) => instance.back.as_ref(),
            AnyType::Attribute(card) => Some(&card.back),
            AnyType::Normal(card) => Some(&card.back),
            AnyType::Class(card) => Some(&card.back),
            AnyType::Unfinished(_) => None?,
            AnyType::Statement(_) => None?,
            AnyType::Event(_) => None?,
        }
    }

    pub async fn delete_card(self) {
        self.card_provider.remove_card(self.id).await;
    }

    pub async fn into_type(self, data: impl Into<AnyType>) -> Card<AnyType> {
        let id = self.id();
        let mut raw: RawCard = self.clone().into();
        raw.data = from_any(data.into());
        let card = Card::from_raw(raw, self.card_provider.clone(), self.recaller.clone()).await;
        self.card_provider.save_card(card).await;
        (*self.card_provider.load(id).await.unwrap()).clone()
    }

    // Call this function every time SavedCard is mutated.
    pub async fn persist(&mut self) {
        info!("persisting card: {}", self.id);
        self.card_provider.save_card(self.clone()).await;
        self.card_provider
            .save_reviews(self.id, self.history.clone())
            .await;
    }

    async fn is_resolved(&self) -> bool {
        for id in self.all_dependencies().await {
            if let Some(card) = self.card_provider.load(id).await {
                if !card.is_finished() {
                    return false;
                }
            }
        }

        true
    }

    pub async fn all_dependents(&self) -> Vec<CardId> {
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

    pub async fn all_dependencies(&self) -> Vec<CardId> {
        tracing::trace!("getting dependencies of: {}", self.id);
        let mut deps = vec![];
        let mut stack = vec![self.id()];

        while let Some(id) = stack.pop() {
            let card = self.card_provider.load(id).await.unwrap();

            if self.id() != id {
                deps.push(id);
            }

            for dep in card.dependency_ids().await {
                stack.push(dep);
            }
        }

        deps
    }

    pub async fn min_rec_recall_rate(&self) -> RecallRate {
        tracing::trace!("min rec recall of {}", self.id);
        self.card_provider.min_rec_recall_rate(self.id).await
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
                    self.card_provider.load(*id).await.unwrap().print().await
                )
            }
            BackSide::List(list) => format!("→ [{}]", {
                let mut res = vec![];
                for id in list {
                    let s = self.card_provider.load(*id).await.unwrap().print().await;
                    res.push(s);
                }

                res.join(", ")
            }),
        })
    }
}

impl<T: CardTrait> Card<T> {
    pub fn history(&self) -> &Reviews {
        &self.history
    }

    pub async fn save_new_reviews(&self) {
        if self.history.is_empty() {
            return;
        }

        self.card_provider
            .save_reviews(self.id, self.history.clone())
            .await;
    }

    fn current_time(&self) -> Duration {
        self.card_provider.time_provider().current_time()
    }

    fn time_passed_since_last_review(&self) -> Option<Duration> {
        if self.current_time() < self.history.0.last()?.timestamp {
            return Duration::default().into();
        }

        Some(self.current_time() - self.history.0.last()?.timestamp)
    }

    pub fn recall_rate_at(&self, current_unix: Duration) -> Option<RecallRate> {
        SimpleRecall.recall_rate(&self.history, current_unix)
    }

    pub fn recall_rate(&self) -> Option<RecallRate> {
        let now = self.current_time();
        self.recaller.recall_rate(&self.history, now)
    }

    pub fn maybeturity(&self) -> Option<f32> {
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

    pub fn maturity(&self) -> f32 {
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
        .unwrap();

        result as f32
    }

    pub async fn print(&self) -> String {
        self.data.display_front().await
    }

    pub fn reviews(&self) -> &Vec<Review> {
        &self.history.0
    }

    #[allow(dead_code)]
    pub fn is_pending(&self) -> bool {
        self.history.is_empty()
    }

    pub fn is_suspended(&self) -> bool {
        self.suspended.is_suspended()
    }

    pub fn time_since_last_review(&self) -> Option<Duration> {
        self.time_passed_since_last_review()
    }

    pub fn id(&self) -> CardId {
        self.id
    }

    pub async fn dependency_ids(&self) -> BTreeSet<CardId> {
        let mut deps = self.dependencies.clone();
        deps.extend(self.data.get_dependencies().await);
        deps
    }

    pub fn lapses(&self) -> u32 {
        self.history.lapses()
    }
}

#[async_trait(?Send)]
impl Matcher for Card<AnyType> {
    #[instrument]
    async fn get_val(&self, key: &str) -> Option<samsvar::Value> {
        match key {
            "front" => json!(self.data.display_front().await),
            "back" => json!(self.display_backside().await.unwrap_or_default()),
            "suspended" => json!(&self.is_suspended()),
            "finished" => json!(&self.is_finished()),
            "resolved" => json!(&self.is_resolved().await),
            "id" => json!(&self.id().to_string()),
            "recall" => json!(self.recall_rate().unwrap_or_default()),
            "stability" => json!(self.maturity()),
            "lapses" => json!(self.lapses()),
            "weeklapses" => json!(self.lapses_last_week()),
            "monthlapses" => json!(self.lapses_last_month()),
            "lastreview" => json!(
                self.time_since_last_review()
                    .unwrap_or_else(|| Duration::MAX)
                    .as_secs_f32()
                    / 86400.
            ),
            "minrecrecall" => {
                json!(self.min_rec_recall_rate().await)
            }
            "minrecstab" => {
                let mut min_recall = usize::MAX;
                let selfs = self.all_dependencies().await;
                for id in selfs {
                    let stab =
                        (self.card_provider.load(id).await.unwrap().maturity() * 1000.) as usize;
                    min_recall = min_recall.min(stab);
                }

                json!(min_recall as f32 / 1000.)
            }
            "dependencies" => json!(self.dependency_ids().await.len()),
            invalid_key => {
                panic!("invalid key: {}", invalid_key);
            }
        }
        .into()
    }
}

#[cfg(test)]
mod tests {

    use super::{AnyType, Card};
    use crate::{
        card_provider::CardProvider, Provider, Recaller, SimpleRecall, TimeGetter, TimeProvider,
    };
    use async_trait::async_trait;
    use speki_dto::{
        BackSide, CType, Config, Cty, RawCard, RawType, Recall, Record, SpekiProvider,
    };
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
        time::Duration,
    };
    use uuid::Uuid;

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

        fn remove(&self, ty: Cty, id: Uuid) {
            let mut lock = self.inner.lock().unwrap();
            match ty {
                Cty::Attribute => &mut lock._attrs,
                Cty::Review => &mut lock.cards,
                Cty::Card => &mut lock.reviews,
            }
            .remove(&id);
        }

        fn save(&self, ty: Cty, id: Uuid, s: String) {
            let mut lock = self.inner.lock().unwrap();
            let map = match ty {
                Cty::Attribute => &mut lock._attrs,
                Cty::Review => &mut lock.cards,
                Cty::Card => &mut lock.reviews,
            };

            let record = Record {
                content: s,
                last_modified: self.time.current_time().as_secs(),
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
    impl SpekiProvider for Storage {
        async fn load_record(&self, id: Uuid, ty: Cty) -> Option<Record> {
            self.get(ty, id)
        }

        async fn load_all_records(&self, ty: Cty) -> HashMap<Uuid, Record> {
            self.get_all(ty)
        }

        async fn save_content(&self, ty: Cty, id: Uuid, content: String) {
            self.save(ty, id, content);
        }

        async fn delete_content(&self, id: Uuid, ty: Cty) {
            self.remove(ty, id);
        }

        async fn load_config(&self) -> Config {
            todo!()
        }
        async fn save_config(&self, _config: Config) {
            todo!()
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
            let provider: Provider = Arc::new(Box::new(Storage::new(timed.clone())));
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

        async fn insert_card(&self) -> Card<AnyType> {
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
