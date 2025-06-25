use either::Either;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::fmt::Display;
use std::fs::{self, hard_link};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::vec::Vec;
use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    hash::{DefaultHasher, Hash, Hasher},
    sync::{Arc, RwLock},
};
use tracing::{info, trace};
use uuid::Uuid;
use walkdir::WalkDir;

pub mod ledger_cache;

#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct PropertyCache<T: LedgerItem> {
    pub property: T::PropertyType,
    pub value: String,
}

impl<T: LedgerItem> PropertyCache<T> {
    pub fn new(property: T::PropertyType, value: String) -> Self {
        Self { property, value }
    }
}

pub enum CardRelation<T: LedgerItem> {
    Reference(ItemReference<T>),
    Property {
        ty: T::PropertyType,
        value: String,
        key: T::Key,
    },
}

/// The way one item references another item
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct ItemReference<T: LedgerItem> {
    pub from: T::Key,
    pub to: T::Key,
    pub ty: T::RefType,
}

impl<T: LedgerItem> ItemReference<T> {
    pub fn new(from: T::Key, to: T::Key, ty: T::RefType) -> Self {
        Self { from, to, ty }
    }
}

/// Represents a way to fetch an item based on either its properites
pub enum TheCacheGetter<T: LedgerItem> {
    ItemRef {
        reversed: bool, // whether it fetches links from the item to other items or the way this item being referenced
        key: T::Key,    // item in question
        ty: Option<T::RefType>, // the way of linking. None means all.
        recursive: bool, // recursively get all cards that link
    },
    Property(PropertyCache<T>),
}

#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct ItemRefCache<T: LedgerItem> {
    pub reftype: T::RefType,
    pub id: T::Key,
}

impl<T: LedgerItem> ItemRefCache<T> {
    pub fn new(reftype: T::RefType, id: T::Key) -> Self {
        Self { reftype, id }
    }
}

pub type CacheKey<T> = Either<PropertyCache<T>, ItemRefCache<T>>;

pub enum CacheGetter<T: LedgerItem> {
    Property(PropertyCache<T>),
    ItemRef {
        itemref: ItemRefCache<T>,
        recursive: bool,
    },
}

use crate::block_chain::BlockChain;

#[derive(Debug, Clone)]
pub enum EventError<T: LedgerItem> {
    Cycle(Vec<T::Key>),
    Invariant(T::Error),
    ItemNotFound,
    DeletingWithDependencies,
}

/// Represents how a ledger mutates or creates an item.
pub trait LedgerItem:
    Serialize + DeserializeOwned + Hash + Clone + Debug + Send + Sync + Eq + PartialEq + 'static
{
    type Key: Copy
        + Eq
        + Hash
        + ToString
        + Debug
        + Serialize
        + DeserializeOwned
        + FromStr
        + Send
        + Sync;
    type Error: Debug;

    /// The different ways an item can reference another item
    type RefType: AsRef<str> + Display + Clone + Hash + PartialEq + Eq + Send + Sync;

    /// Cache regarding property of a card so you get like all the cards that have a certain value or whatever
    type PropertyType: AsRef<str> + Display + Clone + Hash + PartialEq + Eq + Send + Sync;

    /// The type that is responsible for mutating the item and thus create a new genreation
    type Modifier: Clone + Debug + Hash + Serialize + DeserializeOwned + Send + Sync;

    /// Modifies `Self`.
    fn inner_run_event(self, event: Self::Modifier) -> Result<Self, Self::Error>;

    fn verify(self, ledger: &Ledger<Self>) -> Result<Self, EventError<Self>> {
        let ledger = LedgerType::OverRide(OverrideLedger::new(ledger, self.clone()));

        if let Some(cycle) = self.find_cycle(&ledger) {
            return Err(EventError::Cycle(cycle));
        }

        if let Err(e) = self.validate(&ledger) {
            return Err(EventError::Invariant(e));
        }

        for dep in self.recursive_dependents(&ledger) {
            if let Err(e) = dep.validate(&ledger) {
                return Err(EventError::Invariant(e));
            }
        }

        Ok(self)
    }

    /// Modifies `Self` and checks for cycles and invariants.
    fn run_event(
        self,
        event: Self::Modifier,
        ledger: &Ledger<Self>,
        verify: bool,
    ) -> Result<Self, EventError<Self>> {
        let new = match self.inner_run_event(event) {
            Ok(item) => item,
            Err(e) => return Err(EventError::Invariant(e)),
        };

        if verify {
            new.verify(ledger)
        } else {
            Ok(new)
        }
    }

    fn new_default(id: Self::Key) -> Self;

    fn item_id(&self) -> Self::Key;

    fn find_cycle(&self, ledger: &LedgerType<Self>) -> Option<Vec<Self::Key>> {
        fn dfs<T: LedgerItem>(
            current: T::Key,
            ledger: &LedgerType<T>,
            visiting: &mut HashSet<T::Key>,
            visited: &mut HashSet<T::Key>,
            parent: &mut HashMap<T::Key, T::Key>,
            selv: (T::Key, &T),
        ) -> Option<Vec<T::Key>> {
            if !visiting.insert(current.clone()) {
                // cycle start
                let mut path = vec![current.clone()];
                let mut cur = current;
                while let Some(p) = parent.get(&cur) {
                    path.push(p.clone());
                    if *p == current {
                        break;
                    }
                    cur = *p;
                }
                path.reverse();
                return Some(path);
            }

            let dependencies = if selv.0 == current {
                selv.1.dependencies()
            } else {
                ledger.load(current).dependencies()
            };

            for dep in dependencies {
                if visited.contains(&dep) {
                    continue;
                }
                parent.insert(dep.clone(), current.clone());
                if let Some(cycle) = dfs(dep, ledger, visiting, visited, parent, selv) {
                    return Some(cycle);
                }
            }

            visiting.remove(&current);
            visited.insert(current.clone());
            None
        }

        let mut visited = HashSet::new();
        let mut visiting = HashSet::new();
        let mut parent = HashMap::new();

        dfs(
            self.item_id(),
            &ledger,
            &mut visiting,
            &mut visited,
            &mut parent,
            (self.item_id(), self),
        )
    }

    /// Assertions that should hold true. Like invariants with other cards that it references.
    /// called by run_event, if it returns error after an event is run, the event is not applied.
    fn validate(&self, ledger: &LedgerType<Self>) -> Result<(), Self::Error> {
        let _ = ledger;
        Ok(())
    }

    /// List of references to other items, along with the name of the type of reference.
    ///
    /// Used to create a index, like if item A references item B, we cache that item B is referenced by item A,
    /// so that we don't need to search through all the items to find out or store it double in the item itself.
    fn ref_cache(&self) -> HashSet<ItemReference<Self>> {
        Default::default()
    }

    fn dependencies(&self) -> HashSet<Self::Key> {
        self.ref_cache()
            .into_iter()
            .map(|itemref| itemref.to)
            .collect()
    }

    fn recursive_dependent_ids(&self, ledger: &LedgerType<Self>) -> HashSet<Self::Key>
    where
        Self: Sized,
    {
        let mut out: HashSet<Self::Key> = HashSet::new();
        let mut visited: HashSet<Self::Key> = HashSet::new();

        fn visit<T: LedgerItem>(
            key: T::Key,
            ledger: &LedgerType<T>,
            out: &mut HashSet<T::Key>,
            visited: &mut HashSet<T::Key>,
        ) where
            T: Sized,
        {
            if !visited.insert(key) {
                return;
            }

            out.insert(key);

            for dep_key in ledger.dependents(key) {
                visit(dep_key, ledger, out, visited);
            }
        }

        for dep_key in self.dependents(&ledger) {
            visit(dep_key, &ledger, &mut out, &mut visited);
        }

        out
    }

    fn recursive_dependents(&self, ledger: &LedgerType<Self>) -> HashSet<Self>
    where
        Self: Sized,
    {
        let mut out: HashSet<Self> = HashSet::new();
        let mut visited: HashSet<Self::Key> = HashSet::new();

        fn visit<T: LedgerItem>(
            key: T::Key,
            ledger: &LedgerType<T>,
            out: &mut HashSet<T>,
            visited: &mut HashSet<T::Key>,
        ) where
            T: Sized,
        {
            if !visited.insert(key) {
                return;
            }
            let item = ledger.load(key);

            out.insert(item.clone());

            for dep_key in item.dependents(ledger) {
                visit(dep_key, ledger, out, visited);
            }
        }

        for dep_key in self.dependents(&ledger) {
            visit(dep_key, &ledger, &mut out, &mut visited);
        }

        out
    }

    fn dependents(&self, ledger: &LedgerType<Self>) -> HashSet<Self::Key> {
        match ledger {
            LedgerType::OverRide(ledger) => ledger.dependents(self.item_id()),
            LedgerType::Normal(ledger) => ledger.dependents(self.item_id()),
        }
    }

    /// List of defined properties that this item has.
    ///
    /// The property keys are predefined, hence theyre static str
    /// the String is the Value which could be anything.
    /// For example ("suspended", true).
    fn properties_cache(&self, ledger: &Ledger<Self>) -> HashSet<PropertyCache<Self>>
    where
        Self: LedgerItem,
    {
        let _ = ledger;
        Default::default()
    }

    fn listed_cache(&self, ledger: &Ledger<Self>) -> HashMap<CacheKey<Self>, HashSet<Self::Key>> {
        let mut out: HashMap<CacheKey<Self>, HashSet<Self::Key>> = HashMap::default();

        for (key, id) in self.caches(ledger) {
            out.entry(key).or_default().insert(id);
        }

        out
    }

    fn caches(&self, ledger: &Ledger<Self>) -> HashSet<(CacheKey<Self>, Self::Key)>
    where
        Self: LedgerItem,
    {
        trace!("fetching caches for item: {:?}", self.item_id());

        let mut out: HashSet<(CacheKey<Self>, Self::Key)> = Default::default();
        let id = self.item_id();

        for property_cache in self.properties_cache(ledger) {
            out.insert((CacheKey::Left(property_cache), id.clone()));
        }

        for ItemReference { from, to, ty } in self.ref_cache() {
            out.insert((
                CacheKey::Right(ItemRefCache {
                    reftype: ty.clone(),
                    id: to,
                }),
                from,
            ));
        }

        out
    }
}

pub trait TimeProvider {
    fn current_time(&self) -> std::time::Duration;
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

pub type ProviderId = Uuid;
pub type UnixSeconds = u64;
pub type Hashed = String;
pub type StateHash = Hashed;
pub type LedgerHash = Hashed;
pub type CacheHash = Hashed;

#[derive(Clone, Serialize, Deserialize, Debug, Hash)]
#[serde(bound(deserialize = "T: LedgerItem + DeserializeOwned,
                   T::Key: DeserializeOwned"))]
pub struct LedgerEntry<T: LedgerItem> {
    pub previous: Option<Hashed>,
    pub index: usize,
    pub event: TheLedgerEvent<T>,
}

impl<T: LedgerItem> LedgerEntry<T> {
    pub fn save(&self, path: &Path) {
        std::fs::create_dir_all(path).unwrap();
        debug_assert!(path.is_dir());

        let name = format!("{:06}", self.index);
        let path = path.join(name);
        let mut file = std::fs::File::create_new(path).unwrap();
        file.write_all(serde_json::to_string_pretty(&self).unwrap().as_bytes())
            .unwrap();
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, Hash)]
#[serde(bound(deserialize = "T: LedgerItem + DeserializeOwned,
                   T::Key: DeserializeOwned"))]
pub struct TheLedgerEvent<T: LedgerItem> {
    id: T::Key,
    action: TheLedgerAction<T>,
}

impl<T: LedgerItem> TheLedgerEvent<T> {
    pub fn new(id: T::Key, action: TheLedgerAction<T>) -> Self {
        Self { id, action }
    }

    pub fn new_modify(id: T::Key, action: T::Modifier) -> Self {
        Self {
            id,
            action: TheLedgerAction::Modify(action),
        }
    }

    pub fn new_delete(id: T::Key) -> Self {
        Self {
            id,
            action: TheLedgerAction::Delete,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, Hash, PartialEq)]
#[serde(bound(deserialize = "T: LedgerItem + DeserializeOwned"))]
pub enum TheLedgerAction<T: LedgerItem> {
    Create(T),
    Modify(T::Modifier),
    Delete,
}

mod block_chain {
    use std::{
        collections::HashMap,
        path::{Path, PathBuf},
        sync::{Arc, RwLock},
    };

    use crate::{load_file_contents, Hashed, LedgerEntry, LedgerHash, LedgerItem, TheLedgerEvent};

    #[derive(Clone)]
    pub struct BlockChain<T: LedgerItem> {
        cached: Arc<RwLock<Vec<LedgerEntry<T>>>>,
        index_to_hash: Arc<RwLock<HashMap<LedgerHash, usize>>>,
        entries_path: Arc<PathBuf>,
    }

    impl<T: LedgerItem> BlockChain<T> {
        pub fn new(path: PathBuf) -> Self {
            std::fs::create_dir_all(&path).unwrap();
            let cached = Self::load_ledger(&path);
            let mut index_to_hash: HashMap<LedgerHash, usize> = Default::default();

            for entry in &cached {
                index_to_hash.insert(entry.data_hash(), entry.index);
            }

            Self {
                cached: Arc::new(RwLock::new(cached)),
                index_to_hash: Arc::new(RwLock::new(index_to_hash)),
                entries_path: Arc::new(path),
            }
        }

        pub fn chain(&self) -> Vec<LedgerEntry<T>> {
            self.cached.read().unwrap().clone()
        }

        pub fn current_hash(&self) -> Option<Hashed> {
            self.current_head().map(|entry| entry.data_hash())
        }

        fn current_index(&self) -> usize {
            self.cached.read().unwrap().len()
        }

        fn current_head(&self) -> Option<LedgerEntry<T>> {
            self.cached.read().unwrap().last().cloned()
        }

        pub fn save(&self, event: TheLedgerEvent<T>) -> Hashed {
            use std::io::Write;

            let previous = self.current_head();
            let entry = LedgerEntry::new(previous.as_ref(), event);
            let index = self.current_index();
            let ledger_hash = entry.data_hash();

            let name = format!("{:06}", self.current_index());
            let path = &self.entries_path.join(name);
            assert!(!path.exists());
            let mut file = std::fs::File::create_new(path).unwrap();

            let serialized = serde_json::to_string_pretty(&entry).unwrap();
            file.write_all(serialized.as_bytes()).unwrap();
            self.cached.write().unwrap().push(entry);
            self.index_to_hash
                .write()
                .unwrap()
                .insert(ledger_hash.clone(), index);

            self.current_hash().unwrap()
        }

        fn load_ledger(space: &Path) -> Vec<LedgerEntry<T>> {
            dbg!(space);
            let mut foo: Vec<(usize, LedgerEntry<T>)> = {
                let map: HashMap<String, Vec<u8>> = load_file_contents(space);
                let mut foo: Vec<(usize, LedgerEntry<T>)> = Default::default();

                if map.is_empty() {
                    return vec![];
                }

                for (_hash, value) in map.into_iter() {
                    let action: LedgerEntry<T> = serde_json::from_slice(&value).unwrap();
                    let idx = action.index;
                    foo.push((idx, action));
                }

                foo
            };

            foo.sort_by_key(|k| k.0);

            let mut output: Vec<LedgerEntry<T>> = vec![];
            let mut _prev_hash: Option<String> = None;

            for (_, entry) in foo {
                //assert_eq!(entry.previous.clone(), prev_hash);
                //_prev_hash = Some(entry.data_hash());
                output.push(entry);
            }

            output
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TheStateHash {
    ledger_hash: Hashed,
    state_hash: Hashed,
}

pub enum LedgerType<T: LedgerItem> {
    OverRide(OverrideLedger<T>),
    Normal(Ledger<T>),
}

impl<T: LedgerItem> LedgerType<T> {
    pub fn load(&self, key: T::Key) -> T {
        match self {
            LedgerType::OverRide(ledger) => ledger.load(key),
            LedgerType::Normal(ledger) => ledger.load(key),
        }
    }

    pub fn dependents(&self, key: T::Key) -> HashSet<T::Key> {
        match self {
            LedgerType::OverRide(ledger) => ledger.dependents(key),
            LedgerType::Normal(ledger) => ledger.dependents(key),
        }
    }
}

impl<T: LedgerItem> From<Ledger<T>> for LedgerType<T> {
    fn from(value: Ledger<T>) -> Self {
        Self::Normal(value)
    }
}

impl<T: LedgerItem> From<OverrideLedger<T>> for LedgerType<T> {
    fn from(value: OverrideLedger<T>) -> Self {
        Self::OverRide(value)
    }
}

/// Before inserting an item into the state, we want to check if all invariants are still upheld.
/// This struct therefore contain the new item we want to validate.
/// This allows us to pass in this in validation functions so when we check the new/modified items dependencies it'll use the current state for other items,
/// but when it checks invariants based on the new item it'll load the new/modified item from memory
#[derive(Clone)]
pub struct OverrideLedger<T: LedgerItem> {
    inner: Ledger<T>,
    new: T,
    new_id: T::Key,
}

impl<T: LedgerItem> OverrideLedger<T> {
    pub fn new(inner: &Ledger<T>, new: T) -> Self {
        let new_id = new.item_id();

        Self {
            inner: inner.clone(),
            new,
            new_id,
        }
    }

    pub fn load(&self, key: T::Key) -> T {
        if self.new_id == key {
            self.new.clone()
        } else {
            self.inner.load(key)
        }
    }

    pub fn dependencies(&self, key: T::Key) -> HashSet<T::Key> {
        self.load(key).dependencies()
    }

    pub fn dependents(&self, key: T::Key) -> HashSet<T::Key> {
        let mut dependents = self.inner.dependents(key);

        if self.new.dependencies().contains(&key) {
            dependents.insert(self.new_id);
        }

        dependents
    }
}

#[derive(Clone)]
struct ModifyResult<T: LedgerItem> {
    key: T::Key,
    item: Option<T>,
    added_caches: HashSet<(CacheKey<T>, T::Key)>,
    removed_caches: HashSet<(CacheKey<T>, T::Key)>,
}

#[derive(Clone)]
pub struct Ledger<T: LedgerItem> {
    entries: BlockChain<T>,
    properties: Arc<PathBuf>,
    items: Arc<PathBuf>,
    ledger_hash: Arc<PathBuf>,
}

impl<T: LedgerItem> Ledger<T> {
    pub fn new(root: PathBuf) -> Self {
        let root = root.join(Self::item_name());
        let entries = BlockChain::new(root.join("entries"));
        let root = root.join("state");

        let properties = root.join("properties");
        let items = root.join("items");
        let ledger_hash = root.join("applied");

        std::fs::create_dir_all(&properties).unwrap();
        std::fs::create_dir_all(&items).unwrap();

        let selv = Self {
            properties: Arc::new(properties),
            items: Arc::new(items),
            ledger_hash: Arc::new(ledger_hash),
            entries,
        };

        if selv.entries.current_hash() != selv.currently_applied_ledger_hash() {
            selv.apply();
        }

        selv
    }

    fn collect_all_dependents_recursive(
        &self,
        key: T::Key,
        ty: Option<T::RefType>,
        out: &mut HashSet<T::Key>,
        reversed: bool,
    ) {
        let dep_dir = match reversed {
            true => self.dependents_dir(key),
            false => self.dependencies_dir(key),
        };

        let dirs = match ty.clone() {
            Some(ty) => vec![dep_dir.join(ty.to_string())],
            None => fs::read_dir(&dep_dir)
                .unwrap()
                .filter_map(|entry| {
                    let path = entry.unwrap().path();
                    if path.is_dir() {
                        Some(path)
                    } else {
                        None
                    }
                })
                .collect(),
        };

        for dir in dirs {
            for dep_key in Self::item_keys_from_dir(dir) {
                if out.insert(dep_key.clone()) {
                    self.collect_all_dependents_recursive(dep_key, ty.clone(), out, reversed);
                }
            }
        }
    }

    fn load_getter(&self, getter: TheCacheGetter<T>) -> HashSet<T::Key> {
        match getter {
            TheCacheGetter::ItemRef {
                recursive: true,
                reversed,
                key,
                ty,
            } => {
                let mut out = HashSet::new();
                self.collect_all_dependents_recursive(key, ty, &mut out, reversed);
                out
            }
            TheCacheGetter::ItemRef {
                recursive: false,
                reversed: true,
                key,
                ty: Some(ty),
            } => {
                let dep_dir = self.dependents_dir(key);
                let dir = dep_dir.join(ty.to_string());
                Self::item_keys_from_dir(dir)
            }
            TheCacheGetter::ItemRef {
                recursive: false,
                reversed: true,
                key,
                ty: None,
            } => {
                let dep_dir = self.dependents_dir(key);
                Self::item_keys_from_dir_recursive(dep_dir)
            }
            TheCacheGetter::ItemRef {
                recursive: false,
                reversed: false,
                key,
                ty: Some(ty),
            } => {
                let dir = self.dependencies_dir(key).join(ty.to_string());
                Self::item_keys_from_dir(dir)
            }
            TheCacheGetter::ItemRef {
                recursive: false,
                reversed: false,
                key,
                ty: None,
            } => {
                let dir = self.dependencies_dir(key);
                Self::item_keys_from_dir_recursive(dir)
            }
            TheCacheGetter::Property(prop) => self.get_prop_cache(prop),
        }
    }

    fn item_name() -> &'static str {
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
        let name = Self::formatter(any::type_name::<T>());
        let leaked = Box::leak(name.into_boxed_str());
        debug_assert!(cache_lock.insert(type_id, leaked).is_none());
        leaked
    }

    fn formatter(name: &str) -> String {
        name.split("::").last().unwrap().to_lowercase()
    }

    fn verify_all(&self) -> Result<(), EventError<T>> {
        let all = self.load_all();

        let ledger = LedgerType::Normal(self.clone());

        for item in all {
            if let Err(e) = item.validate(&ledger) {
                return Err(EventError::Invariant(e));
            }
        }

        Ok(())
    }

    fn cache_dir(&self, cache: CacheKey<T>) -> PathBuf {
        match cache {
            CacheKey::Left(PropertyCache { property, value }) => {
                self.properties.join(property.to_string()).join(&value)
            }
            CacheKey::Right(ItemRefCache { reftype, id }) => {
                self.dependents_dir(id).join(reftype.to_string())
            }
        }
    }

    fn apply(&self) {
        fs::remove_dir_all(&*self.items).unwrap();
        fs::remove_dir_all(&*self.properties).unwrap();

        fs::create_dir(&*self.items).unwrap();
        fs::create_dir(&*self.properties).unwrap();

        let mut items: HashMap<T::Key, T> = HashMap::default();

        for (idx, entry) in self.entries.chain().into_iter().enumerate() {
            if idx % 50 == 0 {
                dbg!(idx);
            };

            match self.modify_it(entry.event, false, false, false).unwrap() {
                Either::Left(item) => {
                    let key = item.item_id();
                    items.insert(key, item);
                }
                Either::Right(id) => {
                    items.remove(&id);
                }
            }
        }

        self.apply_caches(items);

        self.verify_all().unwrap();

        if let Some(hash) = self.entries.current_hash() {
            self.set_ledger_hash(hash);
        }
    }

    fn set_dependencies(&self, item: &T) {
        let id = item.item_id();
        let depencies_dir = self.dependencies_dir(id);
        fs::remove_dir_all(&depencies_dir).unwrap();
        let depencies_dir = self.dependencies_dir(id); //recreate it

        for ItemReference { from: _, to, ty } in item.ref_cache() {
            let dir = depencies_dir.join(ty.to_string());
            fs::create_dir_all(&dir).unwrap();
            let original = self.item_file(to);
            let link = dir.join(to.to_string());
            hard_link(original, link).unwrap();
        }
    }

    fn apply_caches(&self, items: HashMap<T::Key, T>) {
        info!("applying caches");
        let mut the_caches: HashMap<CacheKey<T>, HashSet<T::Key>> = Default::default();

        info!("fetching caches");
        for item in items.values() {
            for cache in item.caches(self) {
                the_caches.entry(cache.0).or_default().insert(cache.1);
            }
        }

        info!("inserting caches");
        for (idx, (cache, item_keys)) in the_caches.into_iter().enumerate() {
            if idx % 1000 == 0 {
                dbg!(idx);
            }

            for key in item_keys {
                self.insert_cache(cache.clone(), key);
            }
        }
    }

    pub fn load_all(&self) -> HashSet<T> {
        let ids = self.load_ids();
        let mut out = HashSet::with_capacity(ids.len());

        for id in ids {
            out.insert(self.load(id));
        }

        out
    }

    pub fn load_ids(&self) -> HashSet<T::Key> {
        let mut entries: Vec<PathBuf> = vec![];

        for entry in fs::read_dir(self.items.as_path()).unwrap() {
            let entry = entry.unwrap().path();
            entries.push(entry);
        }
        let mut keys: HashSet<T::Key> = HashSet::default();

        for entry in entries {
            for entry in fs::read_dir(entry).unwrap() {
                match entry
                    .unwrap()
                    .file_name()
                    .to_str()
                    .unwrap()
                    .parse::<T::Key>()
                {
                    Ok(key) => keys.insert(key),
                    Err(_) => panic!(),
                };
            }
        }

        keys
    }

    pub fn item_keys_from_dir_recursive(path: PathBuf) -> HashSet<T::Key> {
        if !path.exists() {
            return Default::default();
        }

        let mut out = HashSet::new();

        for entry in WalkDir::new(&path)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
        {
            match entry.file_name().to_str().unwrap().parse::<T::Key>() {
                Ok(key) => {
                    out.insert(key);
                }
                Err(_) => {
                    dbg!(entry.path());
                    panic!("Failed to parse key from file name");
                }
            }
        }

        out
    }

    pub fn item_keys_from_dir(path: PathBuf) -> HashSet<T::Key> {
        if !path.exists() {
            Default::default()
        } else {
            let mut out = HashSet::default();
            for entry in fs::read_dir(&path).unwrap() {
                match entry
                    .unwrap()
                    .file_name()
                    .to_str()
                    .unwrap()
                    .parse::<T::Key>()
                {
                    Ok(key) => out.insert(key),
                    Err(_e) => {
                        dbg!(path);
                        panic!();
                    }
                };
            }
            out
        }
    }

    fn get_cache(&self, key: CacheKey<T>) -> HashSet<T::Key> {
        let path = self.cache_dir(key);
        Self::item_keys_from_dir(path)
    }

    pub fn get_prop_cache(&self, key: PropertyCache<T>) -> HashSet<T::Key> {
        let key = CacheKey::Left(key);
        self.get_cache(key)
    }

    fn collect_item_keys_in_dir(dir: &Path) -> HashSet<T::Key> {
        let mut out: HashSet<T::Key> = Default::default();

        if !dir.is_dir() {
            return Default::default();
        }

        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap().path();
            for entry in fs::read_dir(&entry).unwrap() {
                let entry = entry.unwrap().path();
                let key: T::Key = match entry.file_name().unwrap().to_str().unwrap().parse() {
                    Ok(k) => k,
                    Err(_) => {
                        dbg!(entry);
                        dbg!(&dir);
                        panic!();
                    }
                };
                out.insert(key);
            }
        }

        out
    }

    fn dependencies_dir(&self, key: T::Key) -> PathBuf {
        let p = self.item_dir_from_key(key).join("dependencies");
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn dependents_dir(&self, key: T::Key) -> PathBuf {
        let p = self.item_dir_from_key(key).join("dependents");
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    pub fn get_ref_cache(&self, key: ItemRefCache<T>) -> HashSet<T::Key> {
        let key = CacheKey::Right(key);
        self.get_cache(key)
    }

    pub fn dependencies(&self, key: T::Key) -> HashSet<T::Key> {
        Self::collect_item_keys_in_dir(&self.dependencies_dir(key))
    }

    pub fn dependents(&self, key: T::Key) -> HashSet<T::Key> {
        Self::collect_item_keys_in_dir(&self.dependents_dir(key))
    }

    pub fn recursive_dependencies(&self, key: T::Key) -> HashSet<T::Key>
    where
        Self: Sized,
    {
        let mut out: HashSet<T::Key> = HashSet::new();
        let mut visited: HashSet<T::Key> = HashSet::new();

        fn visit<T: LedgerItem>(
            key: T::Key,
            ledger: &Ledger<T>,
            out: &mut HashSet<T::Key>,
            visited: &mut HashSet<T::Key>,
        ) where
            T: Sized,
        {
            if !visited.insert(key) {
                return;
            }

            out.insert(key);

            for dep_key in ledger.dependencies(key) {
                visit(dep_key, ledger, out, visited);
            }
        }

        for dep_key in self.dependencies(key) {
            visit(dep_key, self, &mut out, &mut visited);
        }

        out
    }
    pub fn recursive_dependents(&self, key: T::Key) -> HashSet<T::Key>
    where
        Self: Sized,
    {
        let mut out: HashSet<T::Key> = HashSet::new();
        let mut visited: HashSet<T::Key> = HashSet::new();

        fn visit<T: LedgerItem>(
            key: T::Key,
            ledger: &Ledger<T>,
            out: &mut HashSet<T::Key>,
            visited: &mut HashSet<T::Key>,
        ) where
            T: Sized,
        {
            if !visited.insert(key) {
                return;
            }

            out.insert(key);

            for dep_key in ledger.dependents(key) {
                visit(dep_key, ledger, out, visited);
            }
        }

        for dep_key in self.dependents(key) {
            visit(dep_key, self, &mut out, &mut visited);
        }

        out
    }

    fn set_ledger_hash(&self, hash: LedgerHash) {
        let mut f = fs::File::create(&*self.ledger_hash).unwrap();
        f.write_all(hash.as_bytes()).unwrap();
    }

    pub fn currently_applied_ledger_hash(&self) -> Option<LedgerHash> {
        fs::read_to_string(&*self.ledger_hash).ok()
    }

    fn remove_dependent(&self, key: T::Key, ty: T::RefType, dependent: T::Key) {
        let mut dependents = self.get_ref_cache(ItemRefCache::new(ty.clone(), key));
        dependents.remove(&dependent);
        let path = self.dependents_dir(key).join(ty.to_string());
        Self::keys_to_file(&path, dependents);
    }

    fn remove_property(&self, key: T::Key, property: T::PropertyType, value: String) {
        let cache = CacheKey::Left(PropertyCache { property, value });
        self.remove_cache(cache, key);
    }

    fn insert_cache(&self, cache: CacheKey<T>, id: T::Key) {
        let path = self.cache_dir(cache);
        fs::create_dir_all(&path).unwrap();
        let original = self.item_file(id);
        let link = path.join(id.to_string());
        hard_link(original, link).unwrap();
    }

    fn remove_cache(&self, cache: CacheKey<T>, id: T::Key) {
        let path = self.cache_dir(cache).join(id.to_string());
        let _res = fs::remove_file(&path);
        debug_assert!(_res.is_ok());
    }

    fn keys_to_file(path: &Path, keys: HashSet<T::Key>) {
        let mut s = String::new();
        for key in keys {
            s.push_str(&format!("{}\n", key.to_string()));
        }

        let mut f = fs::File::create(&path).unwrap();
        f.write(&s.as_bytes()).unwrap();
    }

    fn _modify(
        &self,
        event: TheLedgerEvent<T>,
        verify: bool,
        cache: bool,
    ) -> Result<ModifyResult<T>, EventError<T>> {
        let key = event.id;
        let (old_caches, new_caches, item) = match event.action.clone() {
            TheLedgerAction::Modify(action) => {
                let (old_caches, old_item) = match self.try_load(key) {
                    Some(item) if cache => (item.caches(self), item),
                    Some(item) => (Default::default(), item),
                    None => (Default::default(), T::new_default(key)),
                };
                let modified_item = old_item.run_event(action, self, verify)?;
                let new_caches = modified_item.caches(self);
                (old_caches, new_caches, Some(modified_item))
            }
            TheLedgerAction::Create(mut item) => {
                if verify {
                    item = item.verify(self)?;
                }
                let caches = if cache {
                    item.caches(self)
                } else {
                    Default::default()
                };
                (HashSet::default(), caches, Some(item))
            }
            TheLedgerAction::Delete => {
                let old_item = self.load(key);
                let old_caches = old_item.caches(self);
                (old_caches, Default::default(), None)
            }
        };

        let added_caches = &new_caches - &old_caches;
        let removed_caches = &old_caches - &new_caches;

        Ok(ModifyResult {
            key,
            item,
            added_caches,
            removed_caches,
        })
    }

    fn run_result(&self, res: ModifyResult<T>, cache: bool) -> Result<(), EventError<T>> {
        let ModifyResult {
            key,
            item,
            added_caches,
            removed_caches,
        } = res;

        match item {
            Some(item) => self.save(item),
            None => {
                debug_assert!(added_caches.is_empty());
                self.remove(key);
            }
        }

        if !cache {
            return Ok(());
        }

        for (cache, key) in added_caches {
            self.insert_cache(cache, key);
        }

        for cache in removed_caches {
            let key: T::Key = cache.1;
            match cache.0 {
                CacheKey::Right(ItemRefCache { reftype, id }) => {
                    self.remove_dependent(key, reftype, id);
                }
                CacheKey::Left(PropertyCache { property, value }) => {
                    self.remove_property(key, property, value);
                }
            }
        }

        Ok(())
    }

    pub fn modify_it(
        &self,
        event: TheLedgerEvent<T>,
        save: bool,
        verify: bool,
        cache: bool,
    ) -> Result<Either<T, T::Key>, EventError<T>> {
        let res = self._modify(event.clone(), verify, cache)?;
        self.run_result(res.clone(), cache).unwrap();
        if save {
            let hash = self.entries.save(event);
            self.set_ledger_hash(hash);
        }
        Ok(match res.item {
            Some(item) => Either::Left(item),
            None => Either::Right(res.key),
        })
    }

    pub fn modify(&self, event: TheLedgerEvent<T>) -> Result<(), EventError<T>> {
        self.modify_it(event, true, true, true)?;
        Ok(())
    }

    pub fn remove(&self, key: T::Key) {
        let path = self.item_dir_from_key(key);
        std::fs::remove_dir_all(path).unwrap();
    }

    pub fn save(&self, item: T) {
        let key = item.item_id();
        let item_path = self.item_dir_from_key(key);
        std::fs::create_dir_all(&item_path).unwrap();
        let serialized = serde_json::to_string_pretty(&item).unwrap();
        let mut f = std::fs::File::create(&item_path.join("item")).unwrap();
        use std::io::Write;

        f.write_all(serialized.as_bytes()).unwrap();
        self.set_dependencies(&item);
    }

    fn item_dir_from_key(&self, key: T::Key) -> PathBuf {
        let key = key.to_string();
        let mut chars = key.chars();

        let prefix = if let (Some(ch1), Some(ch2)) = (chars.next(), chars.next()) {
            format!("{}{}", ch1, ch2)
        } else {
            panic!();
        };

        self.items.join(prefix).join(key)
    }

    fn item_file(&self, key: T::Key) -> PathBuf {
        self.item_dir_from_key(key).join("item")
    }

    pub fn try_load(&self, key: T::Key) -> Option<T> {
        if self.item_dir_from_key(key).join("item").exists() {
            Some(self.load(key))
        } else {
            None
        }
    }

    pub fn load(&self, key: T::Key) -> T {
        let path = self.item_file(key);
        let s = fs::read_to_string(&path);
        match s {
            Ok(s) => serde_json::from_str(&s).unwrap(),
            Err(e) => {
                dbg!(e);
                dbg!(key);
                dbg!(path);
                panic!();
            }
        }
    }
}

impl<T: LedgerItem> LedgerEntry<T> {
    fn new(previous: Option<&Self>, event: TheLedgerEvent<T>) -> Self {
        let (index, previous) = match previous {
            Some(e) => (e.index + 1, Some(e.data_hash())),
            None => (0, None),
        };
        Self {
            previous,
            index,
            event,
        }
    }

    fn data_hash(&self) -> Hashed {
        get_hash(self)
    }
}

fn get_hash<T: Hash>(item: &T) -> Hashed {
    let mut hasher = DefaultHasher::new();
    item.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}
