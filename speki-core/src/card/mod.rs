use crate::attribute::Attribute;
use crate::attribute::AttributeId;
use crate::categories::Category;
use crate::common::current_time;
use crate::common::{open_file_with_vim, system_time_as_unix_time};
use crate::reviews::{Recall, Review, Reviews};
use filecash::FsLoad;
use rayon::prelude::*;
use samsvar::json;
use samsvar::Matcher;
use serializing::from_any;
use serializing::into_any;
use serializing::RawCard;
use speki_dto::BackSide;
use speki_dto::CType;
use speki_dto::CardId;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fmt::Debug;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub type RecallRate = f32;

mod back_side;
mod card_types;
mod serializing;

pub use back_side::*;
pub use card_types::*;

#[derive(Ord, PartialOrd, Eq, PartialEq, Hash, Clone, Debug)]
pub struct CardLocation {
    file_name: OsString,
    category: Category,
}

impl CardLocation {
    pub fn new(path: &Path) -> Self {
        let file_name = path.file_name().unwrap().to_owned();
        let category = Category::from_card_path(path);
        Self {
            file_name,
            category,
        }
    }

    fn as_path(&self) -> PathBuf {
        let mut path = self.category.as_path().join(self.file_name.clone());
        path.set_extension("toml");
        path
    }
}

pub trait CardTrait: Debug + Clone {
    fn get_dependencies(&self) -> BTreeSet<CardId>;
    fn display_front(&self) -> String;
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
    fn verify_time(self) -> Self {
        if let Self::TrueUntil(dur) = self {
            if dur < current_time() {
                return Self::False;
            }
        }
        self
    }

    pub fn is_suspended(&self) -> bool {
        !matches!(self, IsSuspended::False)
    }

    pub fn is_not_suspended(&self) -> bool {
        !self.is_suspended()
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

    pub fn set_backside(self, new_back: BackSide) -> Self {
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

impl CardTrait for AnyType {
    fn get_dependencies(&self) -> BTreeSet<CardId> {
        match self {
            AnyType::Instance(card) => card.get_dependencies(),
            AnyType::Normal(card) => card.get_dependencies(),
            AnyType::Unfinished(card) => card.get_dependencies(),
            AnyType::Attribute(card) => card.get_dependencies(),
            AnyType::Class(card) => card.get_dependencies(),
            AnyType::Statement(card) => card.get_dependencies(),
            AnyType::Event(card) => card.get_dependencies(),
        }
    }

    fn display_front(&self) -> String {
        match self {
            AnyType::Instance(card) => card.display_front(),
            AnyType::Normal(card) => card.display_front(),
            AnyType::Unfinished(card) => card.display_front(),
            AnyType::Attribute(card) => card.display_front(),
            AnyType::Class(card) => card.display_front(),
            AnyType::Statement(card) => card.display_front(),
            AnyType::Event(card) => card.display_front(),
        }
    }
}

/// Represents a card that has been saved as a toml file, which is basically anywhere in the codebase
/// except for when youre constructing a new card.
/// Don't save this in containers or pass to functions, rather use the Id, and get new instances of SavedCard from the cache.
/// Also, every time you mutate it, call the persist() method.
#[derive(Clone, Ord, PartialOrd, PartialEq, Eq, Hash, Debug)]
pub struct Card<T: CardTrait + ?Sized> {
    id: CardId,
    data: T,
    dependencies: BTreeSet<CardId>,
    tags: BTreeMap<String, String>,
    history: Reviews,
    location: CardLocation,
    last_modified: Duration,
    created: Duration,
    suspended: IsSuspended,
}

impl<T: CardTrait> std::fmt::Display for Card<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.data.display_front())
    }
}

impl<T: CardTrait> AsRef<CardId> for Card<T> {
    fn as_ref(&self) -> &CardId {
        &self.id
    }
}

impl Card<AttributeCard> {
    pub fn new(attr: AttributeCard, category: &Category) -> Card<AnyType> {
        let raw = RawCard::new_attribute(attr);
        let id = raw.id;
        raw.save_at(&category.as_path());
        Card::from_raw(RawCard::load(id).unwrap())
    }
}

impl Card<AnyType> {
    pub fn lapses_last_month(&self) -> u32 {
        let current_time = current_time();
        let day = Duration::from_secs(86400 * 30);

        self.history.lapses_since(day, current_time)
    }
    pub fn lapses_last_week(&self) -> u32 {
        let current_time = current_time();
        let day = Duration::from_secs(86400 * 7);

        self.history.lapses_since(day, current_time)
    }

    pub fn lapses_last_day(&self) -> u32 {
        let current_time = current_time();
        let day = Duration::from_secs(86400);

        self.history.lapses_since(day, current_time)
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

    /// Loads all the ancestor ancestor classes
    /// for example, king, human male, human
    pub fn load_ancestor_classes(&self) -> Vec<CardId> {
        let mut classes = vec![];
        let mut parent_class = self.parent_class();

        while let Some(class) = parent_class {
            classes.push(class);
            parent_class = Card::from_id(class).unwrap().parent_class();
        }

        classes
    }

    pub fn dependents(id: CardId) -> BTreeSet<CardId> {
        RawCard::load(id.into_inner())
            .unwrap()
            .dependents()
            .into_iter()
            .map(|id| CardId(id))
            .collect()
    }

    pub fn set_ref(mut self, reff: CardId) -> Card<AnyType> {
        let backside = BackSide::Card(reff);
        self.data = self.data.set_backside(backside);
        self.persist();
        self
    }

    // potentially expensive function!
    pub fn from_id(id: CardId) -> Option<Card<AnyType>> {
        Some(Self::from_raw(RawCard::load(id.into_inner())?))
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

    // Call this function every time SavedCard is mutated.
    pub fn persist(&mut self) {
        self.history.save(self.id());
        RawCard::from_card(self.clone()).save();
        *self = Self::from_raw(RawCard::load(self.id().into_inner()).unwrap());
    }

    pub fn from_raw(raw_card: RawCard) -> Card<AnyType> {
        let path = raw_card.path().unwrap();

        let (last_modified, created) = {
            let system_time = std::fs::metadata(&path).unwrap().modified().unwrap();
            let created = std::fs::metadata(&path).unwrap().created().unwrap();
            (
                system_time_as_unix_time(system_time),
                system_time_as_unix_time(created),
            )
        };

        let id = CardId(raw_card.id);

        Card::<AnyType> {
            id,
            data: into_any(raw_card.data),
            dependencies: raw_card
                .dependencies
                .into_iter()
                .map(|id| CardId(id))
                .collect(),
            tags: raw_card.tags,
            history: Reviews::load(id).unwrap_or_default(),
            location: CardLocation::new(&path),
            last_modified,
            created,
            suspended: IsSuspended::from(raw_card.suspended),
        }
    }

    pub fn days_since_created(&self) -> f32 {
        (current_time() - self.created).as_secs_f32() / 86400.
    }

    pub fn save_at(raw_card: RawCard, path: &Path) -> Card<AnyType> {
        let id = raw_card.id;
        raw_card.save_at(path);
        let raw_card = RawCard::load(id).unwrap();
        Self::from_raw(raw_card)
    }

    pub fn new_any(any: AnyType, category: &Category) -> Card<AnyType> {
        let raw_card = RawCard::new(any);
        raw_card.save_at(&category.as_path());
        let raw_card = RawCard::load(raw_card.id).unwrap();
        Self::from_raw(raw_card)
    }

    pub fn new_normal(unfinished: NormalCard, category: &Category) -> Card<AnyType> {
        let raw_card = RawCard::new(unfinished);
        raw_card.save_at(&category.as_path());
        let raw_card = RawCard::load(raw_card.id).unwrap();
        Self::from_raw(raw_card)
    }

    pub fn new_event(class: EventCard, category: &Category) -> Card<AnyType> {
        let raw_card = RawCard::new(class);
        raw_card.save_at(&category.as_path());
        let raw_card = RawCard::load(raw_card.id).unwrap();
        Self::from_raw(raw_card)
    }
    pub fn new_statement(class: StatementCard, category: &Category) -> Card<AnyType> {
        let raw_card = RawCard::new(class);
        raw_card.save_at(&category.as_path());
        let raw_card = RawCard::load(raw_card.id).unwrap();
        Self::from_raw(raw_card)
    }
    pub fn new_class(class: ClassCard, category: &Category) -> Card<AnyType> {
        let raw_card = RawCard::new(class);
        raw_card.save_at(&category.as_path());
        let raw_card = RawCard::load(raw_card.id).unwrap();
        Self::from_raw(raw_card)
    }

    pub fn new_attribute(unfinished: AttributeCard, category: &Category) -> Card<AnyType> {
        let raw_card = RawCard::new(unfinished);
        raw_card.save_at(&category.as_path());
        let raw_card = RawCard::load(raw_card.id).unwrap();
        Self::from_raw(raw_card)
    }

    pub fn new_instance(instance: InstanceCard, category: &Category) -> Card<AnyType> {
        let raw_card = RawCard::new(instance);
        raw_card.save_at(&category.as_path());
        let raw_card = RawCard::load(raw_card.id).unwrap();
        Self::from_raw(raw_card)
    }

    pub fn new_unfinished(unfinished: UnfinishedCard, category: &Category) -> Card<AnyType> {
        let raw_card = RawCard::new(unfinished);
        raw_card.save_at(&category.as_path());
        let raw_card = RawCard::load(raw_card.id).unwrap();
        Self::from_raw(raw_card)
    }

    pub fn load_all_cards() -> Vec<Card<AnyType>> {
        RawCard::load_all()
            .into_par_iter()
            .map(Self::from_raw)
            .collect()
    }

    pub fn load_class_cards() -> Vec<Card<AnyType>> {
        Self::load_all_cards()
            .into_par_iter()
            .filter(|card| card.is_class())
            .collect()
    }

    pub fn load_pending(filter: Option<String>) -> Vec<CardId> {
        Self::load_all_cards()
            .into_par_iter()
            .filter(|card| card.history().is_empty())
            .filter(|card| {
                if let Some(ref filter) = filter {
                    card.eval(filter.clone())
                } else {
                    true
                }
            })
            .map(|card| card.id())
            .collect()
    }

    pub fn load_non_pending(filter: Option<String>) -> Vec<CardId> {
        Self::load_all_cards()
            .into_par_iter()
            .filter(|card| !card.history().is_empty())
            .filter(|card| {
                if let Some(ref filter) = filter {
                    card.eval(filter.clone())
                } else {
                    true
                }
            })
            .map(|card| card.id())
            .collect()
    }

    pub fn rm_dependency(&mut self, dependency: CardId) -> bool {
        let res = self.dependencies.remove(&dependency);
        self.persist();
        res
    }

    pub fn set_dependency(&mut self, dependency: CardId) {
        if self.id() == dependency {
            return;
        }
        self.dependencies.insert(dependency);
        self.persist();
    }

    pub fn edit_with_vim(&self) -> Card<AnyType> {
        let path = self.as_path();
        open_file_with_vim(path.as_path()).unwrap();
        Self::from_raw(RawCard::load(self.id().into_inner()).unwrap())
    }

    pub fn new_review(&mut self, grade: Recall, time: Duration) {
        let review = Review::new(grade, time);
        self.history.add_review(review);
        self.persist();
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

    pub fn into_type(self, data: impl Into<AnyType>) -> Card<AnyType> {
        let id = self.id();
        let mut raw = RawCard::from_card(self);
        raw.data = from_any(data.into());
        raw.save();
        Card::from_id(id).unwrap()
    }
}

impl<T: CardTrait> Card<T> {
    fn history(&self) -> &Reviews {
        &self.history
    }

    pub fn save_new_reviews(&self) {
        if self.history.is_empty() {
            return;
        }
        self.history.save(self.id());
    }

    fn time_passed_since_last_review(&self) -> Option<Duration> {
        if current_time() < self.history.0.last()?.timestamp {
            return Duration::default().into();
        }

        Some(current_time() - self.history.0.last()?.timestamp)
    }

    pub fn recall_rate_at(&self, current_unix: Duration) -> Option<RecallRate> {
        crate::recall_rate::recall_rate(&self.history, current_unix)
    }

    pub fn min_rec_recall_rate(&self) -> Option<RecallRate> {
        let mut recall_rate = self.recall_rate()?;

        for id in self.all_dependencies() {
            let card = Card::from_id(id)?;
            recall_rate = recall_rate.min(card.recall_rate()?);
        }

        Some(recall_rate)
    }

    pub fn recall_rate(&self) -> Option<RecallRate> {
        let now = current_time();
        crate::recall_rate::recall_rate(&self.history, now)
    }

    fn is_resolved(&self) -> bool {
        for id in self.all_dependencies() {
            if let Some(card) = Card::from_id(id) {
                if !card.is_finished() {
                    return false;
                }
            }
        }

        true
    }

    pub fn all_dependencies(&self) -> Vec<CardId> {
        fn inner(id: CardId, deps: &mut Vec<CardId>) {
            let Some(card) = Card::from_id(id) else {
                return;
            };

            for dep in card.dependency_ids() {
                deps.push(dep);
                inner(dep, deps);
            }
        }

        let mut deps = vec![];

        inner(self.id(), &mut deps);

        deps
    }

    pub fn maybeturity(&self) -> Option<f32> {
        use gkquad::single::integral;

        let now = current_time();
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

        let now = current_time();
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

    pub fn print(&self) -> String {
        self.data.display_front()
    }

    pub fn reviews(&self) -> &Vec<Review> {
        &self.history.0
    }

    pub fn last_modified(&self) -> Duration {
        self.last_modified
    }

    pub fn category(&self) -> &Category {
        &self.location.category
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

    pub fn dependency_ids(&self) -> BTreeSet<CardId> {
        let mut deps = self.dependencies.clone();
        deps.extend(self.data.get_dependencies());
        deps
    }

    pub fn as_path(&self) -> PathBuf {
        self.location.as_path()
    }

    /// Checks if corresponding file has been modified after this type got deserialized from the file.
    pub fn is_outdated(&self) -> bool {
        let file_last_modified = {
            let path = self.as_path();
            let system_time = std::fs::metadata(path).unwrap().modified().unwrap();
            system_time_as_unix_time(system_time)
        };

        let in_memory_last_modified = self.last_modified;

        match in_memory_last_modified.cmp(&file_last_modified) {
            Ordering::Less => true,
            Ordering::Equal => false,
            Ordering::Greater => panic!(
            "Card in-memory shouldn't have a last_modified more recent than its corresponding file"
        ),
        }
    }

    pub fn lapses(&self) -> u32 {
        self.history.lapses()
    }
}

impl Matcher for Card<AnyType> {
    fn get_val(&self, key: &str) -> Option<samsvar::Value> {
        match key {
            "front" => json!(&self.data.display_front()),
            "back" => json!(&self
                .back_side()
                .map(|bs| display_backside(bs))
                .unwrap_or_default()),
            "suspended" => json!(&self.is_suspended()),
            "finished" => json!(&self.is_finished()),
            "resolved" => json!(&self.is_resolved()),
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
                let cards = self.all_dependencies();
                for id in cards {
                    let stab = (Card::from_id(id).unwrap().recall_rate().unwrap_or_default()
                        * 1000.) as usize;
                    min_stability = min_stability.min(stab);
                }

                json!(min_stability as f32 / 1000.)
            }
            "minrecstab" => {
                let mut min_recall = usize::MAX;
                let cards = self.all_dependencies();
                for id in cards {
                    let stab = (Card::from_id(id).unwrap().maturity() * 1000.) as usize;
                    min_recall = min_recall.min(stab);
                }

                json!(min_recall as f32 / 1000.)
            }
            "dependencies" => json!(self.dependency_ids().len()),
            "dependents" => {
                let id = self.id();
                let mut count: usize = 0;

                for card in Card::load_all_cards() {
                    if card.dependency_ids().contains(&id) {
                        count += 1;
                    }
                }

                json!(count)
            }
            invalid_key => {
                panic!("invalid key: {}", invalid_key);
            }
        }
        .into()
    }
}
