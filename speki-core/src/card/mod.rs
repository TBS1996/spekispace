use crate::card_provider::CardProvider;
use crate::recall_rate::SimpleRecall;
use crate::reviews::Reviews;
use crate::RecallCalc;
use crate::Recaller;
use crate::TimeGetter;
use samsvar::json;
use samsvar::Matcher;
use serializing::from_any;
use serializing::into_any;
use serializing::into_raw_card;
use speki_dto::BackSide;
use speki_dto::CType;
use speki_dto::CardId;
use speki_dto::RawCard;
use speki_dto::Review;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;
use std::time::Duration;

pub type RecallRate = f32;

mod card_types;
pub(crate) mod serializing;

pub use card_types::*;

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
}

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

    pub async fn from_raw(
        raw_card: RawCard,
        card_provider: CardProvider,
        recaller: Recaller,
    ) -> Card<AnyType> {
        let id = CardId(raw_card.id);

        Card::<AnyType> {
            id,
            data: into_any(raw_card.data, &card_provider),
            dependencies: raw_card.dependencies.into_iter().map(CardId).collect(),
            tags: raw_card.tags,
            history: card_provider.load_reviews(id).await,
            suspended: IsSuspended::from(raw_card.suspended),
            card_provider,
            recaller,
        }
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

    pub fn dependents(_id: CardId) -> BTreeSet<CardId> {
        Default::default()
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
        self.persist().await;
        res
    }
    pub async fn set_dependency(&mut self, dependency: CardId) {
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

    pub async fn into_type(self, data: impl Into<AnyType>) -> Card<AnyType> {
        let id = self.id();
        let mut raw = into_raw_card(self.clone());
        raw.data = from_any(data.into());
        let card = Card::from_raw(raw, self.card_provider.clone(), self.recaller.clone()).await;
        self.card_provider.save_card(card).await;
        self.card_provider.load(id).await.unwrap()
    }

    // Call this function every time SavedCard is mutated.
    pub async fn persist(&mut self) {
        self.card_provider.save_card(self.clone()).await;
        *self = self.card_provider.load(self.id()).await.unwrap();
    }

    pub async fn min_rec_recall_rate(&self) -> Option<RecallRate> {
        let mut recall_rate = self.recall_rate()?;

        for id in self.all_dependencies().await {
            let card = self.card_provider.load(id).await.unwrap();
            recall_rate = recall_rate.min(card.recall_rate()?);
        }

        Some(recall_rate)
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

    pub async fn all_dependencies(&self) -> Vec<CardId> {
        let mut deps = vec![];
        let mut stack = vec![self.id()];

        while let Some(id) = stack.pop() {
            if let Some(card) = self.card_provider.load(id).await {
                if self.id() != id {
                    deps.push(id);
                }

                for dep in card.dependency_ids().await {
                    stack.push(dep);
                }
            }
        }

        deps
    }

    pub async fn display_backside(&self) -> Option<String> {
        Some(match self.back_side()? {
            BackSide::Trivial => format!("…"),
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

use async_trait::async_trait;

#[async_trait(?Send)]
impl Matcher for Card<AnyType> {
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
                let mut min_stability = usize::MAX;
                let selfs = self.all_dependencies().await;
                for id in selfs {
                    let stab = (self
                        .card_provider
                        .load(id)
                        .await
                        .unwrap()
                        .recall_rate()
                        .unwrap_or_default()
                        * 1000.) as usize;
                    min_stability = min_stability.min(stab);
                }

                json!(min_stability as f32 / 1000.)
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
