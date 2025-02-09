use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fmt::Debug,
    time::Duration,
};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tracing::info;
use uuid::Uuid;

pub trait TimeProvider {
    fn current_time(&self) -> std::time::Duration;
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

pub trait Item: Serialize + DeserializeOwned + Sized + Send + Clone + Debug + 'static {
    fn deleted(&self) -> bool;
    fn set_delete(&mut self);

    fn set_last_modified(&mut self, time: Duration);
    fn last_modified(&self) -> Duration;
    fn id(&self) -> Uuid;

    fn as_string(&self) -> String {
        format!("{:?}", self).to_lowercase()
    }

    fn indices(&self) -> Vec<String> {
        self.as_string()
            .split_whitespace()
            .map(String::from)
            .collect()
    }

    fn serialize(&self) -> String {
        toml::to_string(self).unwrap()
    }

    fn deserialize(_id: Uuid, s: String) -> Self {
        if let Ok(item) = toml::from_str(&s) {
            return item;
        } else {
            let x = toml::from_str::<Self>(&s);
            dbg!(
                "{}",
                format!(
                    "unable to deserialize item of type {}: with id: {_id}:  {x:?} input: {s}",
                    Self::identifier()
                )
            );
        };

        if let Ok(item) = serde_json::from_str(&s) {
            return item;
        } else {
            let x = toml::from_str::<Self>(&s);
            panic!(
                "{}",
                format!(
                    "unable to deserialize item of type {}: with id: {_id}:  {x:?} input: {s}",
                    Self::identifier()
                )
            );
        };
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
        let mut map = self.load_all_with_deleted().await;

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
pub trait Indexable<T: Item>: SpekiProvider<T> {
    async fn load_indices(&self, word: String) -> BTreeSet<Uuid>;
    async fn save_indices(&self, word: String, indices: BTreeSet<Uuid>);

    async fn save_index(&self, word: String, id: Uuid) {
        let mut indices = self.load_indices(word.clone()).await;
        indices.insert(id);
        self.save_indices(word, indices).await;
    }

    async fn delete_index(&self, word: String, id: Uuid) {
        let mut indices = self.load_indices(word.clone()).await;
        indices.remove(&id);
        self.save_indices(word, indices).await;
    }

    async fn index_all(&self) {
        for item in self.load_all().await.values() {
            info!("indexing id: {}", item.id());
            for index in item.indices() {
                self.save_index(index, item.id()).await;
            }
        }
    }

    async fn update_item(&self, item: T) {
        info!("updating!!!!!!!!!!!!!!");
        let old_indices = self
            .load_item(item.id())
            .await
            .map(|item| item.indices())
            .unwrap_or_default();

        for word in old_indices {
            self.delete_index(word, item.id()).await;
        }

        for word in item.indices() {
            self.save_index(word, item.id()).await;
        }

        self.save_item(item).await;
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
            Ok(item) => (!item.deleted()).then_some(item),
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

    /// Must not include deleted items.
    async fn load_all(&self) -> HashMap<Uuid, T> {
        info!("loading all for: {:?}", T::identifier());
        let map = self.load_all_records().await;
        let mut outmap = HashMap::new();

        for (key, val) in map {
            let val = <T as Item>::deserialize(key, val.content);
            if !val.deleted() {
                outmap.insert(key, val);
            }
        }
        outmap
    }

    async fn load_all_with_deleted(&self) -> HashMap<Uuid, T> {
        info!("loading all for: {:?}", T::identifier());
        let map = self.load_all_records().await;
        let mut outmap = HashMap::new();

        for (key, val) in map {
            let val = <T as Item>::deserialize(key, val.content);
            outmap.insert(key, val);
        }
        outmap
    }

    async fn delete_item(&self, mut item: T) {
        item.set_delete();
        self.save_item(item).await;
    }

    async fn save_item(&self, mut item: T) {
        item.set_last_modified(self.current_time().await);
        item.set_local_source();
        let record: Record = item.into();
        self.save_record(record).await;
    }
}
