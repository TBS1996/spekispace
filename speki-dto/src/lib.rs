use std::{
    collections::{HashMap, HashSet}, fmt::Debug, hash::{DefaultHasher, Hash, Hasher}, ops::{Deref, DerefMut}, sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard}, time::Duration
};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
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

    fn identifier() -> &'static str {
        std::any::type_name::<Self>().split("::").last().unwrap()
    }

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
pub trait SpekiProvider<T: RunLedger<L>, L: LedgerEvent>: Sync {
    async fn load_content(&self, space: &str, id: &str) -> Option<String>;
    async fn load_all_contents(&self) -> HashMap<String, String>;
    async fn save_content(&self, space: &str, id: String, record: String);
    async fn current_time(&self) -> Duration;
    /// Clear the storage area so we can re-run everything.
    async fn clear_space(&self, space: &str);
    async fn load_ledger(&self) -> Vec<L>;
    async fn save_ledger(&self, event: LedgerEntry<L>);

    async fn load_ids(&self) -> Vec<String> {
        self.load_all_contents().await.into_keys().collect()
    }

    async fn save_cache(&self, key: String, ids: HashSet<String>);
    async fn load_cache(&self, key: &str) -> HashSet<String>;

    async fn save_all_cache(&self) {
        let mut caches : HashMap<String, HashSet<String>> = Default::default();

        for (_, item) in self.load_all().await {
            let id = item.item_id();
            for x in item.caches() {
                caches.entry(x).or_default().insert(id.clone());
            }
        }

        for (key, val) in caches {
            self.save_cache(key, val).await;
        }
    }

    async fn load_item(&self, id: &str) -> Option<T> {
        let record = self.load_content(T::identifier(), id).await?;
        let item = serde_json::from_str(&record).unwrap();
        Some(item)
    }

    async fn hash(&self) -> Hashed {
        let mut state: Vec<(String, T)> = self.load_all().await.into_iter().map(|(key, val)|(key, val)).collect();
        state.sort_by_key(|x|x.0.clone());
        get_hash(&state)
    }

    /// Must not include deleted items.
    async fn load_all(&self) -> HashMap<String, T> {
        info!("loading all for: {:?}", T::identifier());
        let map = self.load_all_contents().await;
        let mut outmap = HashMap::new();

        for (key, val) in map {
            let val: T = serde_json::from_str(&val).unwrap();
            outmap.insert(key, val);
        }

        outmap
    }

    async fn save_item(&self, item: T) {
        let id = item.item_id();
        let s = serde_json::to_string(&item).unwrap();
        self.save_content(T::identifier(), id, s).await;
    }

    fn space_id(&self) -> String {
        format!("{}_ledger", T::identifier())
    }


    async fn clear_state(&self) {
        self.clear_space(T::identifier()).await;
    }

    async fn clear_ledger(&self) {
        self.clear_space(&self.space_id()).await;
    }

    async fn run_event(&self, event: L) {
        info!("running event: {event:?}");

        let item = match self.load_item(&event.id()).await {
            Some(item) => item,
            None => T::new_default(event.id())
        };

        let item = item.run_event(event).unwrap();
        self.save_item(item).await;
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

        let mut state: Vec<(String, T)>  = self.load_all().await.into_iter().map(|(key, val)|(key, val)).collect();
        state.sort_by_key(|k|k.0.clone());
        self.save_all_cache().await;
        get_hash(&state)
    }
}
