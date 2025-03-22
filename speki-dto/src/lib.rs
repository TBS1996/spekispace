use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};
use std::vec::Vec;
use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    hash::{DefaultHasher, Hash, Hasher},
    marker::PhantomData,
    sync::{Arc, RwLock},
};
use tracing::info;
use uuid::Uuid;

pub trait TimeProvider {
    fn current_time(&self) -> std::time::Duration;
}

pub type ProviderId = Uuid;
pub type UnixSeconds = u64;
pub type Hashed = String;

#[derive(Clone)]
pub struct Ledger<T: LedgerItem<E>, E: LedgerEvent> {
    ledger: Arc<RwLock<Vec<LedgerEntry<E>>>>,
    storage: Arc<Box<dyn LedgerStorage<T, E>>>,
}

impl<T: LedgerItem<E>, E: LedgerEvent> Ledger<T, E> {
    pub fn new(storage: Box<dyn LedgerStorage<T, E>>) -> Self {
        Self {
            ledger: Default::default(),
            storage: Arc::new(storage),
        }
    }
}

impl<T: LedgerItem<E>, E: LedgerEvent> Ledger<T, E> {
    pub async fn load(&self, id: &str) -> Option<T> {
        self.storage.load_item(id).await
    }

    pub async fn load_all(&self) -> HashMap<String, T> {
        self.storage.load_all_items().await
    }

    pub async fn load_ids(&self) -> Vec<String> {
        self.storage.load_ids(&self.storage.blob_ns()).await
    }

    pub async fn xsave_ledger(&self, event: E, prev: Option<LedgerEntry<E>>) -> LedgerEntry<E> {
        self.storage.xsave_ledger_entry(event, prev).await
    }

    pub async fn save_ledger(&self, event: E) -> LedgerEntry<E> {
        self.storage.save_ledger_entry(event).await
    }

    pub async fn save_property_cache(&self, property: &str, value: &str, ids: HashSet<String>) {
        self.storage.save_property_cache(property, value, ids).await
    }

    pub async fn load_property_cache(&self, property: &str, value: &str) -> HashSet<String> {
        self.storage.load_property_cache(property, value).await
    }

    pub async fn save_all_cache(&self) {
        let mut caches: HashMap<(&'static str, String), HashSet<E::Key>> = Default::default();
        let mut ref_caches: HashMap<E::Key, HashMap<&'static str, E::Key>> = Default::default();

        for (_, item) in self.storage.load_all_items().await {
            let id = item.item_id();
            for x in item.caches() {
                caches.entry(x).or_default().insert(id);
            }

            for (ty, keys) in item.dep_cache() {
                for key in keys {
                    let x = ref_caches.entry(key).or_default();
                    x.insert(ty, id);
                }
            }
        }

        for ((property, value), id) in caches {
            self.storage
                .save_property_cache(
                    property,
                    &value,
                    id.into_iter().map(|x| x.to_string()).collect(),
                )
                .await;
        }

        for (id, map) in ref_caches {
            for (deptype, reff) in map {
                self.storage
                    .save_refdep(&id.to_string(), deptype, &reff.to_string())
                    .await;
            }
        }
    }

    pub async fn storage_hash(&self) -> Hashed {
        let mut state: Vec<(String, T)> = self
            .storage
            .load_all_items()
            .await
            .into_iter()
            .map(|(key, val)| (key.to_string(), val))
            .collect();
        state.sort_by_key(|x| x.0.clone());
        get_hash(&state)
    }

    pub async fn hash(&self) -> Option<Hashed> {
        self.ledger.read().unwrap().last().map(|last| last.hash())
    }

    pub async fn clear_state(&self) {
        self.storage.clear_space(&[self.storage.item_name()]).await;
    }

    pub async fn run_event(&self, event: E) {
        info!("running event: {event:?}");

        let item = match self.storage.load_item(&event.id().to_string()).await {
            Some(item) => item,
            None => T::new_default(event.id()),
        };

        let id = item.item_id();
        let item = item.run_event(event).unwrap();
        self.storage.save_item(&id.to_string(), item).await;
    }

    pub fn len(&self) -> usize {
        self.ledger.read().unwrap().len()
    }

    pub async fn save_and_run(&self, event: E) {
        self.run_event(event.clone()).await;
        self.save_ledger(event).await;
    }

    pub async fn recompute_state_from_ledger(&self) -> Hashed {
        self.clear_state().await;
        let ledger = self.storage.load_ledger().await;
        info!("length of ledger: {}", ledger.len());
        for event in ledger {
            self.run_event(event.event).await;
        }

        let mut state: Vec<(String, T)> = self
            .storage
            .load_all_items()
            .await
            .into_iter()
            .map(|(key, val)| (key.to_string(), val))
            .collect();
        state.sort_by_key(|k| k.0.clone());
        self.save_all_cache().await;
        get_hash(&state)
    }
}

#[derive(Clone, Serialize, Debug)]
pub struct LedgerEntry<E: LedgerEvent> {
    pub previous: Option<Hashed>,
    pub index: usize,
    pub event: E,
}

impl<'de, E> Deserialize<'de> for LedgerEntry<E>
where
    E: LedgerEvent + DeserializeOwned,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct LedgerEntryHelper<E> {
            previous: Option<Hashed>,
            index: usize,
            event: E,
        }

        let helper = LedgerEntryHelper::deserialize(deserializer)?;
        Ok(LedgerEntry {
            previous: helper.previous,
            index: helper.index,
            event: helper.event,
        })
    }
}

/// Represents a single event in the ledger.
pub trait LedgerEvent:
    Hash + Debug + Clone + Serialize + DeserializeOwned + Send + Sync + 'static
{
    type Key: Copy + Eq + Hash + ToString + Debug;

    fn id(&self) -> Self::Key;

    fn data_hash(&self) -> Hashed {
        get_hash(self)
    }
}

impl<E: LedgerEvent + Serialize + DeserializeOwned> LedgerEntry<E> {
    pub fn new(previous: Option<Hashed>, index: usize, event: E) -> Self {
        Self {
            previous,
            index,
            event,
        }
    }

    pub fn hash(&self) -> Hashed {
        let mut data = self.event.data_hash();
        if let Some(prev) = self.previous.as_ref() {
            data.push_str(prev);
        }

        get_hash(&data)
    }
}


fn get_hash<T: Hash>(item: &T) -> Hashed {
    let mut hasher = DefaultHasher::new();
    item.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

/// Represents how a ledger mutates or creates an item.
pub trait LedgerItem<E: LedgerEvent + Debug>:
    Serialize + DeserializeOwned + Hash + 'static
{
    type Error: Debug;

    fn run_event(self, event: E) -> Result<Self, Self::Error>;

    fn derive_events(&self) -> Vec<E>;

    fn new_default(id: E::Key) -> Self;

    fn item_id(&self) -> E::Key;

    fn dep_cache(&self) -> HashMap<&'static str, HashSet<E::Key>> {
        Default::default()
    }

    fn caches(&self) -> HashSet<(&'static str, String)> {
        Default::default()
    }
}

/// Hmmm, 3 kind of 'caches'
/// self referential stuff, so if A->B, then B should also know about this relationship
/// list of items with a certain property, so, all items that are suspended, or with certain bigrams
/// expensive stuff that can be calculated from the item, like for example the equation for a review history

#[async_trait::async_trait(?Send)]
pub trait LedgerStorage<T: Serialize + DeserializeOwned + 'static, E: LedgerEvent>:
    Storage + ItemStorage<T>
{
    async fn load_refdeps(&self, id: &str, deptype: &str) -> HashSet<String>;
    async fn save_refdep(&self, id: &str, dep_type: &str, reff: &str);

    async fn clear_state(&self) {
        let ns = self.blob_ns();
        self.clear_space(&ns).await;
    }

    async fn load_all_items(&self) -> HashMap<String, T> {
        let ns = self.blob_ns();
        self.xload_all_items(&ns).await
    }

    async fn load_refdep_items(&self, id: &str, deptype: &str) -> HashMap<String, T> {
        let mut out: HashMap<String, T> = Default::default();

        for id in self.load_refdeps(id, deptype).await {
            let item = self.load_item(&id).await.unwrap();
            out.insert(id, item);
        }

        out
    }

    async fn load_property_cache(&self, property: &str, value: &str) -> HashSet<String> {
        let ns = self.property_cache_ns(property);
        self.load_content(&ns, value)
            .await
            .map(|x| serde_json::from_slice(&x).unwrap())
            .unwrap_or_default()
    }

    async fn save_property_cache(&self, property: &str, value: &str, ids: HashSet<String>) {
        let ns = self.property_cache_ns(property);
        let content = serde_json::to_vec(&ids).unwrap();
        self.save_content(&ns, value, &content).await;
    }

    async fn insert_property_cache(&self, property: &str, value: &str, id: String) {
        let mut properties = self.load_property_cache(property, value).await;
        properties.insert(id);
        self.save_property_cache(property, value, properties).await;
    }

    async fn remove_from_property_cache(&self, property: &str, value: &str, id: &str) {
        let mut properties = self.load_property_cache(property, value).await;
        properties.remove(id);
        self.save_property_cache(property, value, properties).await;
    }

    async fn last_ledger_entry(&self) -> Option<LedgerEntry<E>> {
        self.load_ledger().await.last().map(ToOwned::to_owned)
    }

    async fn xsave_ledger_entry(
        &self,
        event: E,
        last_ledger: Option<LedgerEntry<E>>,
    ) -> LedgerEntry<E> {
        let last_hash = last_ledger.as_ref().map(|x| x.hash());
        let new_index = last_ledger.map(|l| l.index).unwrap_or_default() + 1;
        let entry = LedgerEntry {
            previous: last_hash,
            index: new_index,
            event,
        };
        let key = format!("{:06}", new_index);
        let val = serde_json::to_vec(&entry).unwrap();
        let space = self.ledger_ns();
        self.save_content(&space, &key, &val).await;
        entry
    }

    async fn save_ledger_entry(&self, event: E) -> LedgerEntry<E> {
        let last_ledger = self.last_ledger_entry().await;
        self.xsave_ledger_entry(event, last_ledger).await
    }

    async fn load_ledger(&self) -> Vec<LedgerEntry<E>> {
        let space = self.ledger_ns();

        let mut foo: Vec<(usize, LedgerEntry<E>)> = {
            let map: HashMap<String, Vec<u8>> = self.load_all_contents(&space).await;
            let mut foo: Vec<(usize, LedgerEntry<E>)> = Default::default();

            if map.is_empty() {
                return vec![];
            }

            for (hash, value) in map.into_iter() {
                let action: LedgerEntry<E> = serde_json::from_slice(&value).unwrap();
                foo.push((hash.parse().unwrap(), action));
            }

            foo
        };

        foo.sort_by_key(|k| k.0);

        let mut output: Vec<LedgerEntry<E>> = vec![];
        let mut prev_hash: Option<String> = None;

        for (_, entry) in foo {
            assert_eq!(entry.previous.clone(), prev_hash);
            prev_hash = Some(entry.hash());
            output.push(entry);
        }

        output
    }

    async fn load_item(&self, key: &str) -> Option<T> {
        let ns = self.blob_ns();
        self.xload_item(&ns, key).await
    }

    async fn save_item(&self, key: &str, item: T) {
        let ns = self.blob_ns();
        self.xsave_item(&ns, key, &item).await
    }

    fn blob_ns(&self) -> Vec<&str> {
        vec![self.item_name(), "blobs"]
    }

    fn ledger_ns(&self) -> Vec<&str> {
        vec!["ledgers", self.item_name()]
    }

    fn ref_cache_ns(&self) -> Vec<&str> {
        vec![self.item_name(), "ref_cache"]
    }

    fn property_cache_ns<'a>(&self, property: &'a str) -> Vec<&'a str> {
        vec![self.item_name(), "property_cache", property]
    }

    fn formatter(&self, name: &str) -> String {
        name.split("::").last().unwrap().to_lowercase()
    }

    fn item_name(&self) -> &'static str {
        use std::any;
        use std::sync::OnceLock;

        static TYPE_NAME_CACHE: OnceLock<RwLock<HashMap<any::TypeId, &'static str>>> =
            OnceLock::new();

        let cache = TYPE_NAME_CACHE.get_or_init(|| RwLock::new(HashMap::new()));

        let type_id = any::TypeId::of::<T>();

        if let Some(name) = cache.read().unwrap().get(&type_id) {
            return name;
        }

        let mut cache_lock = cache.write().unwrap();
        let name = self.formatter(any::type_name::<T>());
        let leaked = Box::leak(name.into_boxed_str());
        debug_assert!(cache_lock.insert(type_id, leaked).is_none());
        leaked
    }
}

#[async_trait::async_trait(?Send)]
pub trait Storage {
    async fn load_content(&self, space: &[&str], id: &str) -> Option<Vec<u8>>;
    async fn load_all_contents(&self, space: &[&str]) -> HashMap<String, Vec<u8>>;
    async fn save_content(&self, space: &[&str], id: &str, content: &[u8]);
    async fn clear_space(&self, space: &[&str]);

    /// Should be overwritten if there's a faster way.
    async fn load_ids(&self, space: &[&str]) -> Vec<String> {
        self.load_all_contents(space).await.into_keys().collect()
    }

    /// Should be overwritten if there's a faster way.
    async fn exists(&self, space: &[&str], key: &str) -> bool {
        self.load_content(space, key).await.is_some()
    }
}

pub struct BincodeProvider<T: Serialize + DeserializeOwned + 'static> {
    inner: Arc<dyn Storage>,
    _phantom: PhantomData<T>,
}

#[async_trait::async_trait(?Send)]
impl<T: Serialize + DeserializeOwned + 'static> ItemStorage<T> for BincodeProvider<T> {
    async fn xload_item(&self, space: &[&str], id: &str) -> Option<T> {
        let bytes = self.inner.load_content(space, id).await?;
        let x: T = bincode::deserialize(&bytes).unwrap();
        Some(x)
    }

    async fn xload_all_items(&self, space: &[&str]) -> HashMap<String, T> {
        self.inner
            .load_all_contents(space)
            .await
            .into_iter()
            .map(|(key, val)| (key, bincode::deserialize(&val).unwrap()))
            .collect()
    }

    async fn xsave_item(&self, space: &[&str], id: &str, item: &T) {
        let bytes = bincode::serialize(item).unwrap();
        self.inner.save_content(space, id, &bytes).await;
    }
}

#[async_trait::async_trait(?Send)]
pub trait ItemStorage<T> {
    async fn xload_item(&self, space: &[&str], id: &str) -> Option<T>;
    async fn xload_all_items(&self, space: &[&str]) -> HashMap<String, T>;
    async fn xsave_item(&self, space: &[&str], id: &str, item: &T);
}

/// Interface to load/store string encoded content, key-value based system
#[async_trait::async_trait(?Send)]
pub trait TextStorage: Storage {
    async fn load_content(&self, space: &[&str], id: &str) -> Option<String>;
    async fn load_all_contents(&self, space: &[&str]) -> HashMap<String, String>;
    async fn save_content(&self, space: &[&str], id: &str, record: String);
    async fn clear_space(&self, space: &[&str]);

    /// Should be overwritten if there's a faster way.
    async fn load_ids(&self, space: &[&str]) -> Vec<String> {
        todo!()
    }

    /// Should be overwritten if there's a faster way.
    async fn exists(&self, space: &[&str], key: &str) -> bool {
        todo!()
    }
}
