use std::{
    collections::{HashMap, HashSet}, fmt::Debug, hash::{DefaultHasher, Hash, Hasher}, ops::{Deref, DerefMut}, sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard}, time::Duration
};
use std::vec::Vec;
use nonempty::{nonempty, NonEmpty};
use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};
use tracing::info;
use uuid::Uuid;


pub trait TimeProvider {
    fn current_time(&self) -> std::time::Duration;
}

pub type ProviderId = Uuid;
pub type UnixSeconds = u64;
pub type Hashed = String;

#[derive(Clone)]
pub struct Ledger<T: RunLedger<E>, E: LedgerEvent> {
    ledger: Arc<RwLock<Vec<LedgerEntry<E>>>>,
    storage: Arc<Box<dyn Storage<T>>>,
}

impl<T: RunLedger<E>, E: LedgerEvent>Ledger<T, E> {
    pub fn new(storage: Box<dyn Storage<T>>) -> Self {
        Self {
            ledger: Default::default(),
            storage: Arc::new(storage),
        }
    }
}



impl<T: RunLedger<E>, E: LedgerEvent>  Ledger<T, E> {
    pub async fn load(&self, id: &str) -> Option<T> {
        self.storage.load_item(id).await
    }

    pub async fn load_all(&self) -> HashMap<String, T> {
        self.storage.load_all_items().await
    }

    pub async fn load_ids(&self)  -> Vec<String> {
        self.storage.load_item_ids().await
    }

    pub async fn load_ledger(&self) -> Vec<LedgerEntry<E>> {
        let space = self.ledger_space();

        let mut foo: Vec<(usize, LedgerEntry<E>)> = {
            let map: HashMap<String, String> = self.storage.load_all_contents(&space).await;
            let mut foo: Vec<(usize, LedgerEntry<E>)> = Default::default();

            if map.is_empty() {
                return vec![];
            }

            for (hash, value) in map.into_iter(){
                let action: LedgerEntry<E> = serde_json::from_str(&value).unwrap();
                foo.push((hash.parse().unwrap(), action));
            }

            foo
        };

        foo.sort_by_key(|k|k.0);



        let mut output: Vec<LedgerEntry<E>> = vec![];
        let mut prev_hash: Option<String> = None;

        for (_, entry) in foo {
            assert_eq!(entry.previous.clone(), prev_hash);
            prev_hash = Some(entry.hash());
            output.push(entry);
        }

        output
    }

    pub async fn save_ledger(&self, event: LedgerEntry<E>) {
        let key = format!("{:06}", self.len());
        let val = serde_json::to_string(&event).unwrap();
        self.ledger.write().unwrap().push(event);
        let space = self.ledger_space();
        self.storage.save_content(&space, &key, val).await;
    }


    pub async fn save_cache(&self, key: String, ids: HashSet<String>) {
        let space = self.cache_space();
        let content = serde_json::to_string(&ids).unwrap();
        self.storage.save_content(&space, &key, content).await;
    }
    pub async fn load_cache(&self, key: &str) -> HashSet<String> {
        let space = self.cache_space();
        let content = self.storage.load_content(&space, key).await;

        match content {
            Some(s) => serde_json::from_str(&s).unwrap(),
            None => Default::default()
        }
    }

    fn ledger_space(&self) -> String {
        format!("{}_ledger", self.storage.item_name())
    }

    fn cache_space(&self) -> String {
        format!("{}_cache", self.storage.item_name())
    }

    pub async fn save_all_cache(&self) {
        let mut caches : HashMap<String, HashSet<String>> = Default::default();

        for (_, item) in self.storage.load_all_items().await {
            let id = item.item_id();
            for x in item.caches() {
                caches.entry(x).or_default().insert(id.clone());
            }
        }

        for (key, val) in caches {
            self.save_cache(key, val).await;
        }
    }

    pub async fn storage_hash(&self) -> Hashed {
        let mut state: Vec<(String, T)> = self.storage.load_all_items().await.into_iter().map(|(key, val)|(key, val)).collect();
        state.sort_by_key(|x|x.0.clone());
        get_hash(&state)
    }

    pub async fn hash(&self) -> Option<Hashed> {
        self.ledger.read().unwrap().last().map(|last|last.hash())
    }

    pub async fn clear_state(&self) {
        self.storage.clear_space(self.storage.item_name()).await;
    }

    pub async fn clear_ledger(&self) {
        self.storage.clear_space(&self.ledger_space()).await;
    }

    pub async fn run_event(&self, event: E) {
        info!("running event: {event:?}");

        let item = match self.storage.load_item(&event.id()).await {
            Some(item) => item,
            None => T::new_default(event.id())
        };

        let id = item.item_id();
        let item = item.run_event(event).unwrap();
        self.storage.save_item(&id, item).await;
    }

    pub fn len(&self) -> usize {
        self.ledger.read().unwrap().len()
    }

    pub async fn save_and_run(&self, event: E) {
        self.run_event(event.clone()).await;
        let entry: LedgerEntry<E> = LedgerEntry::new(self.hash().await, self.len(), event);
        self.save_ledger(entry).await;
    }


    pub async fn recompute_state_from_ledger(&self) -> Hashed {
        self.clear_state().await;
        let ledger = self.load_ledger().await;
        info!("length of ledger: {}", ledger.len());
        for event in ledger {
            self.run_event(event.event).await;
        }

        let mut state: Vec<(String, T)>  = self.storage.load_all_items().await.into_iter().map(|(key, val)|(key, val)).collect();
        state.sort_by_key(|k|k.0.clone());
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
pub trait LedgerEvent: Hash + Debug + Clone + Serialize + DeserializeOwned + Send + Sync + 'static {
    fn id(&self) -> String;

    fn data_hash(&self) -> Hashed {
        get_hash(self)
    }
}


impl<E: LedgerEvent + Serialize + DeserializeOwned> LedgerEntry<E>{
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
pub trait RunLedger<L: LedgerEvent + Debug>: Serialize + DeserializeOwned + Hash + 'static{
    type Error: Debug;

    fn run_event(self, event: L) -> Result<Self, Self::Error>;

    fn derive_events(&self) -> Vec<L>;

    fn new_default(id: String) -> Self;

    fn item_id(&self) -> String;

    /// when checking for correctness, the items of the returned ids must be checked to verify invariants
    fn invariant_deps(&self) -> Vec<String> {
        vec![]
    }

    fn caches(&self) -> HashSet<String>{
        Default::default()
    }
}




#[async_trait::async_trait(?Send)]
pub trait Storage<T: Serialize + DeserializeOwned + 'static> {
    async fn load_content(&self, space: &str, id: &str) -> Option<String>;
    async fn load_all_contents(&self, space: &str) -> HashMap<String, String>;
    async fn save_content(&self, space: &str, id: &str, record: String);

    async fn load_ids(&self, space: &str) -> Vec<String> {
        self.load_all_contents(space).await.into_keys().collect()
    }


    async fn clear_space(&self, space: &str);

    async fn load_item_ids(&self) -> Vec<String> {
        self.load_ids(self.item_name()).await
    }

    fn item_name(&self) -> &'static str {
        use std::sync::OnceLock;
        use std::any;

        static TYPE_NAME_CACHE: OnceLock<RwLock<HashMap<any::TypeId, &'static str>>> = OnceLock::new();

        let cache = TYPE_NAME_CACHE.get_or_init(|| RwLock::new(HashMap::new()));

        let type_id = any::TypeId::of::<T>();

        if let Some(name) = cache.read().unwrap().get(&type_id) {
            return name;
        }
    
        let mut cache_lock = cache.write().unwrap();
        let name = any::type_name::<T>().split("::").last().unwrap().to_lowercase();
        let leaked = Box::leak(name.into_boxed_str());
        debug_assert!(cache_lock.insert(type_id, leaked).is_none());
        leaked
    }


    async fn load_item(&self, id: &str) -> Option<T> {
        let record = self.load_content(self.item_name(), id).await?;
        let item = serde_json::from_str(&record).unwrap();
        Some(item)
    }

    /// Must not include deleted items.
    async fn load_all_items(&self) -> HashMap<String, T> {
        let map = self.load_all_contents(self.item_name()).await;
        let mut outmap = HashMap::new();

        for (key, val) in map {
            let val: T = serde_json::from_str(&val).unwrap();
            outmap.insert(key, val);
        }

        outmap
    }

    async fn save_item(&self, id: &str, item: T) {
        let s = serde_json::to_string(&item).unwrap();
        self.save_content(self.item_name(), id, s).await;
    }
}



