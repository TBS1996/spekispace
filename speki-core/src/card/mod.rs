use crate::common::current_time;
use crate::recall_rate::SimpleRecall;
use crate::reviews::Reviews;
use crate::FooBar;
use crate::RecallCalc;
use samsvar::json;
use samsvar::Matcher;
use serializing::from_any;
use serializing::from_raw_card;
use serializing::into_any;
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

    pub fn set_backside(self, new_back: BackSide, foobar: &FooBar) -> Self {
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
                foobar: foobar.clone(),
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
#[derive(Clone)]
pub struct Card<T: CardTrait + ?Sized> {
    id: CardId,
    data: T,
    dependencies: BTreeSet<CardId>,
    tags: BTreeMap<String, String>,
    history: Reviews,
    suspended: IsSuspended,
    foobar: FooBar,
}

impl<T: CardTrait + ?Sized> Debug for Card<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = String::new();
        s.push_str(&format!("{:?}\n", self.id));
        s.push_str(&format!("{:?}\n", self.data));

        write!(f, "{}", s)
    }
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

impl Card<AnyType> {
    /// Loads all the ancestor ancestor classes
    /// for example, king, human male, human
    pub fn load_ancestor_classes(&self) -> Vec<CardId> {
        let mut classes = vec![];
        let mut parent_class = self.parent_class();

        while let Some(class) = parent_class {
            classes.push(class);
            parent_class = self.foobar.load_card(class).unwrap().parent_class();
        }

        classes
    }

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

    pub fn from_raw(foobar: FooBar, raw_card: RawCard) -> Card<AnyType> {
        let id = CardId(raw_card.id);

        Card::<AnyType> {
            id,
            data: into_any(raw_card.data, &foobar),
            dependencies: raw_card
                .dependencies
                .into_iter()
                .map(|id| CardId(id))
                .collect(),
            tags: raw_card.tags,
            history: Reviews::load(id),
            suspended: IsSuspended::from(raw_card.suspended),
            foobar,
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

    pub fn set_ref(mut self, reff: CardId) -> Card<AnyType> {
        let backside = BackSide::Card(reff);
        self.data = self.data.set_backside(backside, &self.foobar);
        self.persist();
        self
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
        let mut raw = from_raw_card(self.clone());
        raw.data = from_any(data.into());
        self.foobar.provider.save_card(raw);
        self.foobar.load_card(id).unwrap()
    }

    // Call this function every time SavedCard is mutated.
    pub fn persist(&mut self) {
        let raw = from_raw_card(self.clone());
        self.foobar.provider.save_card(raw);
        *self = self.foobar.load_card(self.id()).unwrap();
    }

    pub fn min_rec_recall_rate(&self) -> Option<RecallRate> {
        let mut recall_rate = self.recall_rate()?;

        for id in self.all_dependencies() {
            let card = self.foobar.load_card(id).unwrap();
            recall_rate = recall_rate.min(card.recall_rate()?);
        }

        Some(recall_rate)
    }

    fn is_resolved(&self) -> bool {
        for id in self.all_dependencies() {
            if let Some(card) = self.foobar.load_card(id) {
                if !card.is_finished() {
                    return false;
                }
            }
        }

        true
    }

    pub fn all_dependencies(&self) -> Vec<CardId> {
        fn inner(id: CardId, deps: &mut Vec<CardId>, foobar: &FooBar) {
            let Some(card) = foobar.load_card(id) else {
                return;
            };

            for dep in card.dependency_ids() {
                deps.push(dep);
                inner(dep, deps, foobar);
            }
        }

        let mut deps = vec![];

        inner(self.id(), &mut deps, &self.foobar);

        deps
    }

    pub fn display_backside(&self) -> Option<String> {
        Some(match self.back_side()? {
            BackSide::Trivial => format!("…"),
            BackSide::Time(time) => format!("🕒 {}", time),
            BackSide::Text(s) => s.to_owned(),
            BackSide::Card(id) => format!("→ {}", self.foobar.load_card(*id).unwrap().print()),
            BackSide::List(list) => format!(
                "→ [{}]",
                list.iter()
                    .map(|id| self.foobar.load_card(*id).unwrap().print())
                    .collect::<Vec<String>>()
                    .join(", ")
            ),
        })
    }
}

impl<T: CardTrait> Card<T> {
    pub fn history(&self) -> &Reviews {
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
        SimpleRecall.recall_rate(&self.history, current_unix)
    }

    pub fn recall_rate(&self) -> Option<RecallRate> {
        let now = current_time();
        self.foobar.recaller.recall_rate(&self.history, now)
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

    pub fn lapses(&self) -> u32 {
        self.history.lapses()
    }
}

impl Matcher for Card<AnyType> {
    fn get_val(&self, key: &str) -> Option<samsvar::Value> {
        match key {
            "front" => json!(self.data.display_front()),
            "back" => json!(self.display_backside().unwrap_or_default()),
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
                let selfs = self.all_dependencies();
                for id in selfs {
                    let stab = (self
                        .foobar
                        .load_card(id)
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
                let selfs = self.all_dependencies();
                for id in selfs {
                    let stab = (self.foobar.load_card(id).unwrap().maturity() * 1000.) as usize;
                    min_recall = min_recall.min(stab);
                }

                json!(min_recall as f32 / 1000.)
            }
            "dependencies" => json!(self.dependency_ids().len()),
            "dependents" => {
                let id = self.id();
                let mut count: usize = 0;

                for card in self.foobar.load_all_cards() {
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
