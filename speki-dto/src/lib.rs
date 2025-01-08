use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fmt::Debug,
    str::FromStr,
    time::Duration,
};

use omtrent::TimeStamp;
use serde::{de::DeserializeOwned, ser::SerializeSeq, Deserialize, Deserializer, Serialize};
use serde_json::Value;
use tracing::info;
use uuid::Uuid;

pub trait TimeProvider {
    fn current_time(&self) -> std::time::Duration;
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct Collection {
    pub id: Uuid,
    pub name: String,
    pub cards: Vec<CardId>,
    pub last_modified: Duration,
    pub deleted: bool,
    pub source: ModifiedSource,
}

impl Item for Collection {
    fn deleted(&self) -> bool {
        self.deleted
    }

    fn set_delete(&mut self) {
        self.deleted = true;
    }

    fn set_last_modified(&mut self, time: Duration) {
        self.last_modified = time;
    }

    fn last_modified(&self) -> Duration {
        self.last_modified
    }

    fn id(&self) -> Uuid {
        self.id
    }

    fn identifier() -> &'static str {
        "collections"
    }

    fn source(&self) -> ModifiedSource {
        self.source
    }

    fn set_source(&mut self, source: ModifiedSource) {
        self.source = source;
    }
}

impl Item for History {
    fn last_modified(&self) -> Duration {
        self.reviews
            .iter()
            .max()
            .map(|rev| rev.timestamp)
            .unwrap_or_default()
    }

    fn set_last_modified(&mut self, _time: Duration) {}

    fn source(&self) -> ModifiedSource {
        self.source
    }

    fn set_source(&mut self, source: ModifiedSource) {
        self.source = source;
    }

    fn id(&self) -> Uuid {
        self.id
    }

    fn serialize(&self) -> String {
        toml::to_string(self).unwrap()
    }

    fn identifier() -> &'static str {
        "reviews"
    }

    fn merge(mut self, other: Self) -> Option<MergeInto<Self>>
    where
        Self: Sized,
    {
        debug_assert!(self.id == other.id);

        let selflen = self.reviews.len();
        let otherlen = other.reviews.len();

        if selflen == otherlen {
            return None;
        }

        let merged = {
            self.merge_into(other);
            self
        };

        let mergedlen = merged.len();

        Some(if mergedlen == selflen {
            MergeInto::Right(merged)
        } else if mergedlen == otherlen {
            MergeInto::Left(merged)
        } else {
            MergeInto::Both(merged)
        })
    }

    fn deleted(&self) -> bool {
        false
    }

    fn set_delete(&mut self) {}

    fn deserialize(id: Uuid, s: String) -> Self {
        parse_history(s, id)
    }
}

impl Item for AttributeDTO {
    fn last_modified(&self) -> Duration {
        self.last_modified
    }

    fn set_last_modified(&mut self, time: Duration) {
        self.last_modified = time;
    }

    fn set_source(&mut self, source: ModifiedSource) {
        self.source = source;
    }

    fn source(&self) -> ModifiedSource {
        self.source
    }

    fn id(&self) -> Uuid {
        self.id
    }

    fn serialize(&self) -> String {
        toml::to_string(self).unwrap()
    }

    fn identifier() -> &'static str {
        "attributes"
    }

    fn deleted(&self) -> bool {
        self.deleted
    }

    fn set_delete(&mut self) {
        self.deleted = true;
    }

    fn deserialize(_id: Uuid, s: String) -> Self {
        toml::from_str(&s).unwrap()
    }
}

impl Item for RawCard {
    fn last_modified(&self) -> Duration {
        self.last_modified
    }

    fn set_last_modified(&mut self, time: Duration) {
        self.last_modified = time;
    }

    fn set_source(&mut self, source: ModifiedSource) {
        self.source = source;
    }

    fn source(&self) -> ModifiedSource {
        self.source
    }

    fn deserialize(_id: Uuid, s: String) -> Self {
        toml::from_str(&s).unwrap()
    }

    fn id(&self) -> Uuid {
        self.id
    }

    fn serialize(&self) -> String {
        toml::to_string(self).unwrap()
    }

    fn identifier() -> &'static str {
        "cards"
    }

    fn deleted(&self) -> bool {
        self.deleted
    }

    fn set_delete(&mut self) {
        self.deleted = true;
    }
}

impl<T: Item> From<T> for Record {
    fn from(value: T) -> Self {
        value.into_record()
    }
}

/// Whether the item was modified in the current provider or elsewhere
/// and if elsewhere, when was it inserted into this provider?
#[derive(
    Hash, Copy, Default, Clone, Debug, Serialize, Deserialize, Ord, Eq, PartialEq, PartialOrd,
)]
pub enum ModifiedSource {
    #[default]
    /// This item was created in the current provider.
    Local,
    /// This item was not created with current provider but from another one.
    External {
        /// The provider where this is saved as local
        from: ProviderId,
        /// The time this item was inserted into the current provider
        inserted: Duration,
    },
}

pub trait Item: Serialize + DeserializeOwned + Sized + Send + Clone + 'static {
    fn deleted(&self) -> bool;
    fn set_delete(&mut self);

    fn set_last_modified(&mut self, time: Duration);
    fn last_modified(&self) -> Duration;
    fn id(&self) -> Uuid;

    fn serialize(&self) -> String {
        toml::to_string(self).unwrap()
    }

    fn deserialize(_id: Uuid, s: String) -> Self {
        toml::from_str(&s).unwrap()
    }

    fn identifier() -> &'static str;
    fn source(&self) -> ModifiedSource;
    fn set_source(&mut self, source: ModifiedSource);

    fn set_local_source(&mut self) {
        self.set_source(ModifiedSource::Local);
    }

    fn set_external_source(&mut self, id: ProviderId, now: Duration) {
        let source = ModifiedSource::External {
            from: id,
            inserted: now,
        };
        self.set_source(source);
    }

    /// Returns whether the returned value should be saved to self, other, both, or none.
    fn merge(self, right_item: Self) -> Option<MergeInto<Self>>
    where
        Self: Sized,
    {
        let left_item = self;

        let res = if left_item.deleted() && !right_item.deleted() {
            MergeInto::Both(left_item)
        } else if !left_item.deleted() && right_item.deleted() {
            MergeInto::Both(right_item)
        } else if left_item.last_modified() > right_item.last_modified() {
            MergeInto::Right(left_item)
        } else if left_item.last_modified() < right_item.last_modified() {
            MergeInto::Left(right_item)
        } else {
            return None;
        };

        Some(res)
    }

    fn into_record(self) -> Record
    where
        Self: Sized,
    {
        let id = self.id().to_string();
        let last_modified = self.last_modified().as_secs();
        let content = Item::serialize(&self);
        let inserted = match self.source() {
            ModifiedSource::Local => None,
            ModifiedSource::External { inserted, .. } => Some(inserted.as_secs()),
        };

        Record {
            id,
            content,
            last_modified,
            inserted,
        }
    }
}

pub enum MergeInto<T> {
    Left(T),
    Right(T),
    Both(T),
}

#[derive(Ord, PartialOrd, Eq, PartialEq, Hash, Clone, Debug, Deserialize, Serialize)]
pub struct History {
    id: Uuid,
    reviews: Vec<Review>,
    #[serde(default)]
    source: ModifiedSource,
}

impl History {
    pub fn inner(&self) -> &Vec<Review> {
        &self.reviews
    }

    pub fn last(&self) -> Option<Review> {
        self.reviews.last().cloned()
    }

    pub fn lapses_since(&self, dur: Duration, current_time: Duration) -> u32 {
        let since = current_time - dur;
        self.reviews
            .iter()
            .fold(0, |lapses, review| match review.grade {
                Recall::None | Recall::Late => {
                    if review.timestamp < since {
                        0
                    } else {
                        lapses + 1
                    }
                }
                Recall::Some | Recall::Perfect => 0,
            })
    }

    pub fn lapses(&self) -> u32 {
        self.reviews
            .iter()
            .fold(0, |lapses, review| match review.grade {
                Recall::None | Recall::Late => lapses + 1,
                Recall::Some | Recall::Perfect => 0,
            })
    }

    pub fn time_since_last_review(&self, current_unix: Duration) -> Option<Duration> {
        let last = self.reviews.last()?;
        Some(current_unix - last.timestamp)
    }

    pub fn len(&self) -> usize {
        self.reviews.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn new(id: CardId) -> Self {
        Self {
            id,
            reviews: Default::default(),
            source: Default::default(),
        }
    }

    pub fn push(&mut self, review: Review) {
        self.reviews.push(review);
    }

    pub fn insert_many(&mut self, reviews: impl IntoIterator<Item = Review>) {
        self.reviews.extend(reviews);
        self.reviews.sort_by_key(|r| r.timestamp);
        self.reviews.dedup();
    }

    pub fn merge_into(&mut self, other: Self) {
        self.insert_many(other.reviews);
    }
}

#[derive(Ord, PartialOrd, Eq, PartialEq, Hash, Clone, Debug, Default, Deserialize, Serialize)]
pub struct Review {
    // When (unix time) did the review take place?
    pub timestamp: Duration,
    // Recall grade.
    pub grade: Recall,
    // How long you spent before attempting recall.
    pub time_spent: Duration,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Record {
    pub id: String,
    pub content: String,
    pub last_modified: UnixSeconds,
    pub inserted: Option<UnixSeconds>,
}

pub type ProviderId = Uuid;
pub type UnixSeconds = u64;

#[async_trait::async_trait(?Send)]
pub trait Syncable<T: Item>: Sync + SpekiProvider<T> {
    async fn save_id(&self, id: ProviderId);
    async fn load_id_opt(&self) -> Option<ProviderId>;

    async fn provider_id(&self) -> ProviderId {
        if let Some(id) = self.load_id_opt().await {
            return id;
        }

        let new_id = ProviderId::new_v4();
        self.save_id(new_id).await;

        self.load_id_opt().await.unwrap()
    }

    async fn update_sync_info(&self, other: ProviderId, now: Duration);
    async fn last_sync(&self, other: ProviderId) -> Duration;

    async fn load_new(&self, other_id: ProviderId) -> HashMap<Uuid, T> {
        let last_sync = self.last_sync(other_id).await;
        let new_items = self.load_all_after(last_sync).await;
        info!(
            "new items from {self_id} that are new for {other_id} of type {ty} since {last_sync}: {qty}",
            self_id = self.provider_id().await,
            last_sync = last_sync.as_secs(),
            qty = new_items.len(),
            ty = T::identifier(),
        );
        new_items
    }

    async fn load_all_after(&self, not_before: Duration) -> HashMap<Uuid, T> {
        let mut map = self.load_all().await;

        info!(
            "loaded {} {}, retaining those last modified after {:?}",
            map.len(),
            T::identifier(),
            not_before
        );

        map.retain(|_, val| match val.source() {
            ModifiedSource::Local => val.last_modified().as_secs() > (not_before.as_secs() + 1),
            ModifiedSource::External { inserted, .. } => {
                inserted.as_secs() > (not_before.as_secs() + 1)
            }
        });

        map
    }

    async fn save_from_sync(&self, from: ProviderId, records: Vec<Record>, now: Duration) {
        let ty = T::identifier();
        info!(
            "updating {qty} {ty} in {self_id} from {from}",
            qty = records.len(),
            self_id = self.provider_id().await,
        );
        self.save_records(records).await;
        self.update_sync_info(from, now).await;
    }

    /// Syncs the state between two providers.
    ///
    /// Must not be called in parallel if the save_id function overwrites previous id.
    /// If it only saves if empty then you can call in parallel.
    /// Otherwise it might generate different Ids for different types.
    async fn sync(self, other: impl Syncable<T>)
    where
        Self: Sized,
    {
        use futures::future::join;

        let (left, right) = (self, other);

        let ty = T::identifier();

        let (left_id, right_id) = join(left.provider_id(), right.provider_id()).await;

        let now = async {
            let (left_time, right_time) = join(left.current_time(), right.current_time()).await;

            if left_time.abs_diff(right_time) > Duration::from_secs(60) {
                let msg = format!("time between {ty:?} providers too great. time from {left_id}: {leftsec}, time from {right_id}: {rightsec}", leftsec = left_time.as_secs(), rightsec = right_time.as_secs());
                panic!("{msg}");
            } else {
                left_time.max(right_time)
            }
        };

        info!("starting sync of: {ty} between {left_id} and {right_id}");

        let mergeres = async {
            let (mut new_from_left, mut new_from_right) =
                join(left.load_new(right_id), right.load_new(left_id)).await;

            let ids: HashSet<Uuid> = new_from_left
                .keys()
                .chain(new_from_right.keys())
                .cloned()
                .collect();

            ids.into_iter()
                .filter_map(|id| {
                    let left_item = new_from_left.remove(&id);
                    let right_item = new_from_right.remove(&id);

                    match (left_item, right_item) {
                        (None, None) => unreachable!("ID should exist in at least one map"),
                        (None, Some(right_item)) => Some(MergeInto::Left(right_item)),
                        (Some(left_item), None) => Some(MergeInto::Right(left_item)),
                        (Some(left_item), Some(right_item)) => left_item.merge(right_item),
                    }
                })
                .collect::<Vec<MergeInto<T>>>()
        };

        let (mergeres, now) = join(mergeres, now).await;

        let (left_update, right_update) = {
            let mut left_update = vec![];
            let mut right_update = vec![];

            for res in mergeres {
                match res {
                    MergeInto::Left(mut item) => {
                        item.set_external_source(right_id, now);
                        left_update.push(item.into_record());
                    }
                    MergeInto::Right(mut item) => {
                        item.set_external_source(left_id, now);
                        right_update.push(item.into_record());
                    }
                    MergeInto::Both(mut item) => {
                        item.set_external_source(left_id, now);
                        right_update.push(item.clone().into_record());

                        item.set_external_source(right_id, now);
                        left_update.push(item.into_record());
                    }
                }
            }

            (left_update, right_update)
        };

        join(
            left.save_from_sync(right_id, left_update, now),
            right.save_from_sync(left_id, right_update, now),
        )
        .await;

        info!("finished sync of: {ty} between {left_id} and {right_id}");
    }
}

#[async_trait::async_trait(?Send)]
pub trait SpekiProvider<T: Item>: Sync {
    async fn load_record(&self, id: Uuid) -> Option<Record>;
    async fn load_all_records(&self) -> HashMap<Uuid, Record>;
    async fn save_record(&self, record: Record);

    async fn current_time(&self) -> Duration;

    async fn save_records(&self, records: Vec<Record>) {
        for record in records {
            self.save_record(record).await;
        }
    }

    async fn load_ids(&self) -> Vec<Uuid> {
        self.load_all_records().await.into_keys().collect()
    }

    async fn load_item(&self, id: Uuid) -> Option<T> {
        let record = self.load_record(id).await?;
        match toml::from_str::<T>(&record.content) {
            Ok(item) => Some(item),
            Err(e) => {
                tracing::error!(
                    "error deserializing {:?} with id {}: {e:?}",
                    T::identifier(),
                    id
                );
                None
            }
        }
    }

    async fn load_all(&self) -> HashMap<Uuid, T> {
        info!("loading all for: {:?}", T::identifier());
        let map = self.load_all_records().await;
        let mut outmap = HashMap::new();

        for (key, val) in map {
            let val = <T as Item>::deserialize(key, val.content);
            outmap.insert(key, val);
        }
        outmap
    }

    async fn save_item(&self, mut item: T) {
        item.set_last_modified(self.current_time().await);
        item.set_local_source();
        let record: Record = item.into();
        self.save_record(record).await;
    }
}

fn parse_history(s: String, id: Uuid) -> History {
    if let Ok(history) = toml::from_str(&s) {
        history
    } else {
        History {
            id,
            reviews: legacy_parse_history(s),
            source: Default::default(),
        }
    }
}

fn legacy_parse_history(s: String) -> Vec<Review> {
    let mut reviews = vec![];
    for line in s.lines() {
        let (timestamp, grade) = line.split_once(' ').unwrap();
        let timestamp = Duration::from_secs(timestamp.parse().unwrap());
        let grade = Recall::from_str(grade).unwrap();
        let review = Review {
            timestamp,
            grade,
            time_spent: Duration::default(),
        };
        reviews.push(review);
    }

    reviews.sort_by_key(|r| r.timestamp);
    reviews
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Config;

#[derive(Serialize, Deserialize, Debug, Clone, Default, Copy, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CType {
    Instance,
    #[default]
    Normal,
    Unfinished,
    Attribute,
    Class,
    Statement,
    Event,
}

pub type CardId = Uuid;

#[derive(Ord, PartialOrd, Eq, Hash, PartialEq, Debug, Clone)]
pub enum BackSide {
    Text(String),
    Card(CardId),
    List(Vec<CardId>),
    Time(TimeStamp),
    Trivial, // Answer is obvious, used when card is more of a dependency anchor
    Invalid, // A reference card was deleted
}

impl Default for BackSide {
    fn default() -> Self {
        Self::Text(Default::default())
    }
}

impl From<String> for BackSide {
    fn from(s: String) -> Self {
        if let Ok(uuid) = Uuid::parse_str(&s) {
            Self::Card(uuid)
        } else if let Some(timestamp) = TimeStamp::from_string(s.clone()) {
            Self::Time(timestamp)
        } else if s.as_str() == Self::INVALID_STR {
            Self::Invalid
        } else {
            Self::Text(s)
        }
    }
}

impl BackSide {
    pub const INVALID_STR: &'static str = "__INVALID__";

    pub fn is_empty_text(&self) -> bool {
        if let Self::Text(s) = self {
            s.is_empty()
        } else {
            false
        }
    }

    pub fn invalidate_if_has_ref(&mut self, dep: CardId) {
        let has_ref = match self {
            BackSide::Card(card_id) => card_id == &dep,
            BackSide::List(vec) => vec.contains(&dep),
            BackSide::Text(_) => false,
            BackSide::Time(_) => false,
            BackSide::Trivial => false,
            BackSide::Invalid => false,
        };

        if has_ref {
            *self = Self::Invalid;
        }
    }

    pub fn is_ref(&self) -> bool {
        matches!(self, Self::Card(_))
    }

    pub fn as_card(&self) -> Option<CardId> {
        if let Self::Card(card) = self {
            Some(*card)
        } else {
            None
        }
    }

    pub fn to_string(&self) -> String {
        let mut s = serde_json::to_string(self).unwrap();
        s.remove(0);
        s.pop();
        s
    }

    pub fn dependencies(&self) -> BTreeSet<CardId> {
        let mut set = BTreeSet::default();
        match self {
            BackSide::Text(_) => {}
            BackSide::Card(card_id) => {
                let _ = set.insert(*card_id);
            }
            BackSide::List(vec) => {
                set.extend(vec.iter());
            }
            BackSide::Time(_) => {}
            BackSide::Trivial => {}
            BackSide::Invalid => {}
        }

        set
    }
}

impl<'de> Deserialize<'de> for BackSide {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;

        match value {
            Value::Array(arr) => {
                let mut ids = Vec::new();
                for item in arr {
                    if let Value::String(ref s) = item {
                        if let Ok(uuid) = Uuid::parse_str(s) {
                            ids.push(uuid);
                        } else {
                            return Err(serde::de::Error::custom("Invalid UUID in array"));
                        }
                    } else {
                        return Err(serde::de::Error::custom("Expected string in array"));
                    }
                }
                Ok(BackSide::List(ids))
            }
            Value::Bool(_) => Ok(BackSide::Trivial),
            Value::String(s) => Ok(s.into()),
            _ => Err(serde::de::Error::custom("Expected a string or an array")),
        }
    }
}

impl Serialize for BackSide {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match *self {
            BackSide::Trivial => serializer.serialize_bool(false),
            BackSide::Invalid => serializer.serialize_str(Self::INVALID_STR),
            BackSide::Time(ref t) => serializer.serialize_str(&t.serialize()),
            BackSide::Text(ref s) => serializer.serialize_str(s),
            BackSide::Card(ref id) => serializer.serialize_str(&id.to_string()),
            BackSide::List(ref ids) => {
                let mut seq = serializer.serialize_seq(Some(ids.len()))?;
                for id in ids {
                    seq.serialize_element(&id.to_string())?;
                }
                seq.end()
            }
        }
    }
}

fn is_false(flag: &bool) -> bool {
    !flag
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct RawType {
    pub ty: CType,
    pub front: Option<String>,
    pub back: Option<BackSide>,
    pub class: Option<Uuid>,
    pub instance: Option<Uuid>,
    pub attribute: Option<Uuid>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub parent_event: Option<Uuid>,
}

impl RawType {
    pub fn class(&self) -> Option<Uuid> {
        self.class.clone()
    }
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct RawCard {
    pub id: Uuid,
    #[serde(flatten)]
    pub data: RawType,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub dependencies: BTreeSet<Uuid>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tags: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub suspended: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub deleted: bool,
    #[serde(default)]
    pub last_modified: Duration,
    #[serde(default)]
    pub source: ModifiedSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttributeDTO {
    pub pattern: String,
    pub id: AttributeId,
    pub class: CardId,
    pub back_type: Option<CardId>,
    #[serde(default)]
    pub last_modified: Duration,
    #[serde(default)]
    pub deleted: bool,
    #[serde(default)]
    pub source: ModifiedSource,
}

pub type AttributeId = Uuid;

#[derive(
    Ord, PartialOrd, Eq, PartialEq, Hash, Deserialize, Serialize, Debug, Default, Clone, Copy,
)]
#[serde(rename_all = "lowercase")]
pub enum Recall {
    // No recall, not even when you saw the answer.
    #[default]
    None,
    // No recall, but you remember the answer when you read it.
    Late,
    // Struggled but you got the answer right or somewhat right.
    Some,
    // No hesitation, perfect recall.
    Perfect,
}

impl Recall {
    pub fn get_factor(&self) -> f32 {
        match self {
            Recall::None => 0.1,
            Recall::Late => 0.25,
            Recall::Some => 2.,
            Recall::Perfect => 3.,
        }
    }
}

impl std::str::FromStr for Recall {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "1" => Ok(Self::None),
            "2" => Ok(Self::Late),
            "3" => Ok(Self::Some),
            "4" => Ok(Self::Perfect),
            _ => Err(()),
        }
    }
}
