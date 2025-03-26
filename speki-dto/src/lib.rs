use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};
use snapstore::fs::SnapFs;
use snapstore::{CacheKey, PropertyCacheKey, RefCacheKey, SnapStorage};
use std::fs::{self, create_dir_all, hard_link};
use std::io::Write;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
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
pub type StateHash = Hashed;
pub type LedgerHash = Hashed;

#[derive(Clone)]
pub struct Ledger<T: LedgerItem<E>, E: LedgerEvent> {
    ledger: Arc<RwLock<Vec<LedgerEntry<E>>>>,
    snap: SnapFs,
    root: Arc<PathBuf>,
    _phantom: PhantomData<T>,
}

impl<T: LedgerItem<E>, E: LedgerEvent> Ledger<T, E> {
    pub fn new(storage: Box<dyn LedgerStorage<T, E>>) -> Self {
        let root = Arc::new(PathBuf::from("/home/tor/spekifs").join(storage.item_name()));
        let snap = SnapFs::new((*root).clone());
        let selv = Self {
            ledger: Default::default(),
            snap,
            root,
            _phantom: PhantomData,
        };

        let ledger = Self::load_ledger(&selv.ledger_path());
        *selv.ledger.write().unwrap() = ledger;
        selv
    }
}


pub fn load_file_contents(dir: &Path) -> HashMap<String, Vec<u8>> {
    let mut map = HashMap::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(content) = fs::read(entry.path()) {
                let key = entry.file_name().into_string().unwrap();
                map.insert(key, content);
            }
        }
    }
    map
}

impl<T: LedgerItem<E>, E: LedgerEvent> Ledger<T, E> {
    pub async fn load(&self, id: &str) -> Option<T> {
        let state = self.state_hash().await;
        match self.snap.get(&state, id) {
            Some(item) => serde_json::from_slice(&item).unwrap(),
            None => None,
        }
    }

    pub async fn save_ledger(&self, event: E) -> LedgerEntry<E> {
        let hash = self.ledger.read().unwrap().last().map(|e|e.hash());
        let idx = self.ledger.read().unwrap().last().map(|e|e.index).unwrap_or_default();
        let entry = LedgerEntry::new(hash, idx + 1, event);
        let hash = entry.hash();
        let vec = serde_json::to_vec(&entry).unwrap();
        let path = self.ledger_path().join(hash);
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(&vec).unwrap();

        let gen_path = self.gen_path().join(format!("{:06}", idx));
        symlink(&path, &gen_path).unwrap();

        entry
    }

    fn state_map_path(&self) -> PathBuf {
        self.root.join("states")
    }

    fn gen_path(&self) -> PathBuf {
        self.root.join("generations")
    }

    fn ledger_path(&self) -> PathBuf {
        self.root.join("entries")
    }

    pub async fn get_cache(&self, cache_key: impl Into<CacheKey>) -> Vec<String> {
        let hash = self.state_hash().await;
        self.snap.get_cache(&hash, &cache_key.into())
    }

    fn get_state(&self, ledger_hash: &str) -> Option<StateHash> {
        let path = self.state_map_path().join(ledger_hash);
        if !path.exists() {
            None
        } else {
            Some(fs::read_link(&path).unwrap().file_name().unwrap().to_str().unwrap().to_string())
        }
    }

    pub async fn state_hash(&self)  -> StateHash {
        let ledger = self.ledger.read().unwrap();
        let ledger = ledger.iter().rev();
        let mut unapplied_entries = vec![];

        let mut last_applied = None;

        for entry in ledger {
            let ledger_hash = entry.hash();
            if let Some(state_hash) = self.get_state(&ledger_hash) {
                last_applied = Some(state_hash);
                break;
            } else {
                unapplied_entries.push(entry);
            }
        }

        let mut last_applied = last_applied;

        while let Some(entry)  = unapplied_entries.pop() {
           let state_hash = self.run_event(entry.event.clone(), last_applied.as_deref()).await;
           self.save_ledger_state(&entry.hash(), &state_hash);
           last_applied = Some(state_hash);
        }

        last_applied.unwrap_or_default()
    }

    /// creates a symlink from the hash of a ledger to its corresponding state
    fn save_ledger_state(&self, ledger_hash: &str, state_hash: &str) {
        let sp = self.snap.the_full_blob_path(state_hash);
        let ledger_path = self.state_map_path().join(ledger_hash);
        symlink(ledger_path, sp).unwrap();
    }

    pub fn normalize_ledger(&self) {
        use std::io::Write;

        let ledger = Self::load_ledger(&PathBuf::from("/home/tor/spekifs/ledgers/history"));
        let new_path = PathBuf::from("/home/tor/ledgertest");
        let generations = new_path.join("generations");
        let state = new_path.join("states");
        let entries = new_path.join("entries");

        fs::create_dir_all(&generations).unwrap();
        fs::create_dir_all(&state).unwrap();
        fs::create_dir_all(&entries).unwrap();

        for entry in ledger {
            let gen_path = generations.join(format!("{:06}", entry.index));

            let entry_path = entries.join(entry.hash());
            let content = serde_json::to_vec(&entry).unwrap();
            let mut f = fs::File::create(&entry_path).unwrap();
            f.write_all(&content).unwrap();

            hard_link(&entry_path, &gen_path).unwrap();
        }
    }


    fn load_ledger(space: &Path) -> Vec<LedgerEntry<E>> {
        let mut foo: Vec<(usize, LedgerEntry<E>)> = {
            let map: HashMap<String, Vec<u8>> = load_file_contents(&space);
            let mut foo: Vec<(usize, LedgerEntry<E>)> = Default::default();

            if map.is_empty() {
                return vec![];
            }

            for (idx, value) in map.into_iter() {
                let action: LedgerEntry<E> = serde_json::from_slice(&value).unwrap();
                foo.push((idx.parse().unwrap(), action));
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

    pub async fn load_all(&self) -> HashMap<String, T> {
        let hash = self.state_hash().await;
        self.snap.get_all(&hash).into_iter().map(|(key, val)| (key, serde_json::from_slice(&val).unwrap())).collect()
    }

    pub async fn load_ids(&self) -> Vec<String> {
        self.snap.get_all(&self.state_hash().await).into_keys().collect()
    }

    pub async fn hash(&self) -> Option<Hashed> {
        self.ledger.read().unwrap().last().map(|last| last.hash())
    }

    pub async fn clear_state(&self) {
        todo!()
    }

    /// Clones the current state, modifies it with the new entry, and returns the hash of the new state.
    pub async fn run_event(&self, event: E, state_hash: Option<&str>) -> StateHash{
        info!("running event: {event:?}");

        let mut new_item = true;
        let item = match state_hash {
            Some(hash) => {
                match self.snap.get(hash, &event.id().to_string()).map(|v|serde_json::from_slice(&v).unwrap()) {
                    Some(item) => {
                        new_item = false;
                        item
                    },
                    None => T::new_default(event.id()),
                }
            },
            None => T::new_default(event.id()),
        };

        let old_cache = if !new_item { item.caches() } else {Default::default()};

        let id = item.item_id();
        let item = item.run_event(event).unwrap();
        let new_caches = item.caches();
        let item = serde_json::to_vec(&item).unwrap();
        let mut state_hash = self.snap.save(state_hash, &id.to_string(), item);

        let added_caches = new_caches.difference(&old_cache);
        for (cache_key, id) in added_caches {
            state_hash = self.snap.insert_cache(&state_hash, cache_key, id);
        }

        let removed_caches = old_cache.difference(&new_caches);
        for (cache_key, id) in removed_caches {
            state_hash = self.snap.remove_cache(&state_hash, cache_key, id);
        }

        state_hash
    }

    pub fn len(&self) -> usize {
        self.ledger.read().unwrap().len()
    }

    pub async fn recompute_state_from_ledger(&self) -> Hashed {
        self.state_hash().await
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

    fn new_default(id: E::Key) -> Self;

    fn item_id(&self) -> E::Key;

    /// List of references to other items, along with the name of the type of reference.
    /// 
    /// Used to create a index, like if item A references item B, we cache that item B is referenced by item A, 
    /// so that we don't need to search through all the items to find out or store it double in the item itself.
    fn ref_cache(&self) -> HashMap<&'static str, HashSet<E::Key>> {
        Default::default()
    }

    /// List of defined properties that this item has.
    /// 
    /// The property keys are predefined, hence theyre static str
    /// the String is the Value which could be anything. 
    /// For example ("suspended", true).
    fn properties_cache(&self) -> HashSet<(&'static str, String)> {
        Default::default()
    }

    fn caches(&self) -> HashSet<(CacheKey, String)> {
        let mut out: HashSet<(CacheKey, String)> = Default::default();
        let id = self.item_id().to_string();

        for (property, value) in self.properties_cache()  {
            let key = PropertyCacheKey {
                property,
                value,
            };
            out.insert((CacheKey::Property(key), id.clone()));
        }

        for (reftype, ids) in self.ref_cache() {
            let key = RefCacheKey {
                reftype,
                id: id.to_string(),
            };

            for ref_id in ids {
                out.insert((CacheKey::ItemRef(key.clone()), ref_id.to_string()));
            }
        }

        out
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
        todo!()
        //self.get_all().into_iter().map(|(key, val)| (key, serde_json::from_slice(&val).unwrap()) ).collect()
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
        todo!()
        //self.get(key).map(|val|serde_json::from_slice(&val).unwrap())
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
