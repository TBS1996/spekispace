use std::{
    collections::{HashMap, HashSet}, fmt::Debug, hash::{DefaultHasher, Hash, Hasher}, ops::{Deref, DerefMut}, sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard}, time::Duration
};

use serde::{de::DeserializeOwned, Serialize};
use tracing::info;
use uuid::Uuid;


pub trait TimeProvider {
    fn current_time(&self) -> std::time::Duration;
}

pub type ProviderId = Uuid;
pub type UnixSeconds = u64;


#[derive(Clone)]
pub struct LedgerEntry<E: LedgerEvent + Serialize + DeserializeOwned> {
    pub timestamp: Duration,
    pub event: E,
}

impl<E: LedgerEvent + Serialize + DeserializeOwned> LedgerEntry<E>{
    pub fn new(timestamp: Duration, event: E) -> Self {
        Self {
            timestamp, event,
        }
    }
}

/// Represents a single event in the ledger.
pub trait LedgerEvent: Debug + Clone + Serialize + DeserializeOwned + Send + Sync + 'static{
    fn id(&self) -> String;
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

pub type Hashed = String;

fn get_hash<T: Hash>(item: &T) -> Hashed {
    let mut hasher = DefaultHasher::new();
    item.hash(&mut hasher);
    format!("{:x}", hasher.finish()) 
}


#[async_trait::async_trait(?Send)]
pub trait Storage<T: Serialize + DeserializeOwned + 'static> {
    async fn load_content(&self, space: &str, id: &str) -> Option<String>;
    async fn load_all_contents(&self, space: &str) -> HashMap<String, String>;
    async fn save_content(&self, space: &str, id: &str, record: String);

    async fn load_ids(&self, space: &str) -> Vec<String> {
        self.load_all_contents(space).await.into_keys().collect()
    }

    async fn load_item_ids(&self) -> Vec<String> {
        self.load_ids(self.item_name()).await
    }

    fn item_name(&self) -> &'static str {
        std::any::type_name::<T>().split("::").last().unwrap()
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



#[async_trait::async_trait(?Send)]
pub trait LedgerProvider<T: RunLedger<L>, L: LedgerEvent>: Storage<T> {
    async fn current_time(&self) -> Duration;
    /// Clear the storage area so we can re-run everything.
    async fn clear_space(&self, space: &str);

    async fn load_ledger(&self) -> Vec<L> {
        let space = self.ledger_space();

        let map: HashMap<String, String> = self.load_all_contents(&space).await;

        let mut foo: Vec<LedgerEntry<L>> = vec![];

        for (key, value) in map.iter(){
            let key: u64 = key.parse().unwrap();
            let action: L = serde_json::from_str(&value).unwrap();
            let timestamp = Duration::from_micros(key);
            let event: LedgerEntry<L> = LedgerEntry::new(timestamp, action);
            foo.push(event);
        }

        foo.sort_by_key(|k|k.timestamp);
        foo.into_iter().map(|e| e.event).collect()

    }
    async fn save_ledger(&self, event: LedgerEntry<L>) {
        let key = event.timestamp.as_micros().to_string();
        let val = serde_json::to_string(&event.event).unwrap();
        let space = <Self as LedgerProvider<T, L>>::ledger_space(self);
        self.save_content(&space, &key, val).await;
    }


    async fn save_cache(&self, key: String, ids: HashSet<String>) {
        let space = self.cache_space();
        let content = serde_json::to_string(&ids).unwrap();
        self.save_content(&space, &key, content).await;
    }
    async fn load_cache(&self, key: &str) -> HashSet<String> {
        let space = self.cache_space();
        let content = self.load_content(&space, key).await;

        match content {
            Some(s) => serde_json::from_str(&s).unwrap(),
            None => Default::default()
        }
    }

    fn ledger_space(&self) -> String {
        format!("{}_ledger", self.item_name())
    }

    fn cache_space(&self) -> String {
        format!("{}_space", self.item_name())
    }

    async fn save_all_cache(&self) {
        let mut caches : HashMap<String, HashSet<String>> = Default::default();

        for (_, item) in self.load_all_items().await {
            let id = item.item_id();
            for x in item.caches() {
                caches.entry(x).or_default().insert(id.clone());
            }
        }

        for (key, val) in caches {
            self.save_cache(key, val).await;
        }
    }

    async fn hash(&self) -> Hashed {
        let mut state: Vec<(String, T)> = self.load_all_items().await.into_iter().map(|(key, val)|(key, val)).collect();
        state.sort_by_key(|x|x.0.clone());
        get_hash(&state)
    }

    async fn clear_state(&self) {
        self.clear_space(self.item_name()).await;
    }

    async fn clear_ledger(&self) {
        self.clear_space(&self.ledger_space()).await;
    }

    async fn run_event(&self, event: L) {
        info!("running event: {event:?}");

        let item = match self.load_item(&event.id()).await {
            Some(item) => item,
            None => T::new_default(event.id())
        };

        let id = item.item_id();
        let item = item.run_event(event).unwrap();
        self.save_item(&id, item).await;
    }

    async fn save_and_run(&self, event: L, now: Duration) {
        self.run_event(event.clone()).await;
        let entry: LedgerEntry<L> = LedgerEntry::new(now, event);
        self.save_ledger(entry).await;
    }


    async fn recompute_state_from_ledger(&self) -> Hashed {
        self.clear_state().await;
        let ledger = self.load_ledger().await;
        info!("length of ledger: {}", ledger.len());
        for event in ledger {
            self.run_event(event).await;
        }

        let mut state: Vec<(String, T)>  = self.load_all_items().await.into_iter().map(|(key, val)|(key, val)).collect();
        state.sort_by_key(|k|k.0.clone());
        self.save_all_cache().await;
        get_hash(&state)
    }
}
