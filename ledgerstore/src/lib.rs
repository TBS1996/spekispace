use either::Either;
use rayon::prelude::*;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use snapstore::fs::{CacheFs, Content, SnapFs};
use snapstore::mem::SnapMem;
use snapstore::{HashAndContents, Key};
use std::fmt::Display;
use std::fs::{self};
use std::io::Write;
use std::os::unix::fs::symlink;
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

pub mod ledger_cache;

pub use snapstore::CacheKey;

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

    /// Modifies `Self` and checks for cycles and invariants.
    fn run_event(
        self,
        event: Self::Modifier,
        ledger: &FixedLedger<Self>,
    ) -> Result<Self, EventError<Self>> {
        let new = match self.inner_run_event(event) {
            Ok(item) => item,
            Err(e) => return Err(EventError::Invariant(e)),
        };

        let ledger = NewFixedLedger::new(ledger.clone(), new.clone());

        if let Some(cycle) = new.find_cycle(&ledger) {
            return Err(EventError::Cycle(cycle));
        }

        if let Err(e) = new.validate(&ledger) {
            return Err(EventError::Invariant(e));
        }

        for dep in new.recursive_dependents(&ledger) {
            if let Err(e) = dep.validate(&ledger) {
                return Err(EventError::Invariant(e));
            }
        }

        Ok(new)
    }

    fn new_default(id: Self::Key) -> Self;

    fn item_id(&self) -> Self::Key;

    fn find_cycle(&self, ledger: &NewFixedLedger<Self>) -> Option<Vec<Self::Key>> {
        fn dfs<T: LedgerItem>(
            current: T::Key,
            ledger: &FixedLedger<T>,
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
                ledger.load(current).unwrap().dependencies()
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
            &ledger.fixed,
            &mut visiting,
            &mut visited,
            &mut parent,
            (self.item_id(), self),
        )
    }

    /// Assertions that should hold true. Like invariants with other cards that it references.
    /// called by run_event, if it returns error after an event is run, the event is not applied.
    fn validate(&self, ledger: &NewFixedLedger<Self>) -> Result<(), Self::Error> {
        let _ = ledger;
        Ok(())
    }

    /// List of references to other items, along with the name of the type of reference.
    ///
    /// Used to create a index, like if item A references item B, we cache that item B is referenced by item A,
    /// so that we don't need to search through all the items to find out or store it double in the item itself.
    fn ref_cache(&self) -> HashMap<Self::RefType, HashSet<Self::Key>> {
        Default::default()
    }

    fn dependencies(&self) -> HashSet<Self::Key> {
        self.ref_cache().into_values().flatten().collect()
    }

    fn recursive_dependents(&self, ledger: &NewFixedLedger<Self>) -> HashSet<Self>
    where
        Self: Sized,
    {
        let mut out: HashSet<Self> = HashSet::new();
        let mut visited: HashSet<Self::Key> = HashSet::new();

        fn visit<T: LedgerItem>(
            key: T::Key,
            ledger: &FixedLedger<T>,
            out: &mut HashSet<T>,
            visited: &mut HashSet<T::Key>,
        ) where
            T: Sized,
        {
            if !visited.insert(key) {
                return;
            }

            if let Some(item) = ledger.load(key) {
                out.insert(item.clone());

                for dep_key in item.dependents(ledger) {
                    visit(dep_key, ledger, out, visited);
                }
            }
        }

        for dep_key in self.dependents(&ledger.fixed) {
            visit(dep_key, &ledger.fixed, &mut out, &mut visited);
        }

        out
    }

    fn dependents(&self, ledger: &FixedLedger<Self>) -> HashSet<Self::Key> {
        ledger.get_dependents(self.item_id()).into_iter().collect()
    }

    /// List of defined properties that this item has.
    ///
    /// The property keys are predefined, hence theyre static str
    /// the String is the Value which could be anything.
    /// For example ("suspended", true).
    fn properties_cache(&self, ledger: &FixedLedger<Self>) -> HashSet<(Self::PropertyType, String)>
    where
        Self: LedgerItem,
    {
        let _ = ledger;
        Default::default()
    }

    fn caches(
        &self,
        ledger: &FixedLedger<Self>,
    ) -> HashSet<(CacheKey<Self::PropertyType, Self::RefType>, String)>
    where
        Self: LedgerItem,
    {
        info!("fetching caches for item: {:?}", self.item_id());

        let mut out: HashSet<(CacheKey<Self::PropertyType, Self::RefType>, String)> =
            Default::default();
        let id = self.item_id().to_string();

        for (property, value) in self.properties_cache(ledger) {
            out.insert((CacheKey::Property { property, value }, id.clone()));
        }

        for (reftype, ids) in self.ref_cache() {
            for ref_id in ids {
                out.insert((
                    CacheKey::Dependents {
                        id: ref_id.to_string(),
                    },
                    id.to_string(),
                ));
                out.insert((
                    CacheKey::ItemRef {
                        reftype: reftype.clone(),
                        id: ref_id.to_string(),
                    },
                    id.to_string(),
                ));
            }
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

#[derive(Clone)]
pub struct NewFixedLedger<T: LedgerItem> {
    pub fixed: FixedLedger<T>,
    pub new: Arc<T>,
}

impl<T: LedgerItem> NewFixedLedger<T> {
    pub fn new(ledger: FixedLedger<T>, new: T) -> Self {
        Self {
            fixed: ledger,
            new: Arc::new(new),
        }
    }

    pub fn load(&self, id: T::Key) -> Option<T> {
        if self.new.item_id() == id {
            Some(Arc::unwrap_or_clone(self.new.clone()))
        } else {
            self.fixed.load(id)
        }
    }

    pub fn get_dependents(&self, id: T::Key) -> Vec<T::Key> {
        if self.new.item_id() == id {
            vec![]
        } else {
            self.fixed.get_dependents(id)
        }
    }
}

/*
some idea..

instead of returning T, it'll return something like Stored<T>,

this wraps T internally and implements deref -> T so you have access to it directly, no mutation though which you shouldn't be able to do anyway
since "mutation" is done by creating new snapshots instead

should be a macro to specify that a method returns a KeyRef, this will automatically take care of the reverse-index caching

so if you have a dependencies() method you can annotate it and it'll update the cache with reverse-index of dependencies
then this Saved<T> will have a generated method that let's you get the dependents

for annotating methods of refkeys, it'll accept any methods that return either Vec<ItemKey>, ItemKey, or Option<ItemKey>, i guess

this Stored<T> will also allow you to instead of inserting new events by passing in an itemkey and the action, simply pass in the action directly in Stored<T>

and internally it'll use the Ledger struct and pass in its own itemkey to update the ledger
this method should consume Self and return a new version of itself.

note: should there be a way to communicate to other in-memory Stored<T> that they are no longer valid?
if so, it can be tricky, cause then the other ones inmemory might have invalid reads like if it's deleted or smth

also if you annotate like a dependencies() method, should we override it and create a similar method that instead of returning the keyrefs, returns Stored<T> instead?
since we can load them from Ledger?


maybe a way to have different ledgers communicate sorta, and declare that the id in two
ledgers represent the same thing? like how review ID refer to cardId ?
*/

/// A ledger fixed to a certain hash.
#[derive(Clone)]
pub struct FixedLedger<T: LedgerItem> {
    inner: Either<Ledger<T>, SnapMem<T::Key>>,
    hash: Option<StateHash>,
}

impl<T: LedgerItem> FixedLedger<T> {
    pub fn load(&self, id: T::Key) -> Option<T> {
        match &self.inner {
            Either::Left(ledger) => {
                let state = self.hash.as_ref()?;
                match ledger.snap.get(state, id) {
                    Some(item) => serde_json::from_slice(&item).unwrap(),
                    None => None,
                }
            }
            Either::Right(mem) => mem
                .get(&id.to_string())
                .map(|data| serde_json::from_slice(&data).unwrap()),
        }
    }

    pub fn get_dependents(&self, id: T::Key) -> Vec<T::Key> {
        let Some(hash) = self.hash.as_deref() else {
            return vec![];
        };

        match &self.inner {
            Either::Left(ledger) => ledger
                .get_the_cache(hash, &CacheKey::Dependents { id: id.to_string() })
                .into_iter()
                .map(|k| match k.parse() {
                    Ok(k) => k,
                    Err(_) => panic!(),
                })
                .collect(),
            Either::Right(_) => todo!(),
        }
    }
}

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
    pub fn new(id: T::Key, action: T::Modifier) -> Self {
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

#[derive(Clone)]
pub struct Ledger<T: LedgerItem> {
    ledger: Arc<RwLock<Vec<LedgerEntry<T>>>>,
    snap: SnapFs<T::Key>,
    cache: CacheFs<T::PropertyType, T::RefType>,
    root: Arc<PathBuf>,
    gc_keep: usize,
    all_paths: Arc<RwLock<HashMap<StateHash, Arc<HashSet<Content>>>>>,
}

impl<T: LedgerItem> Ledger<T> {
    /// Map from a state hash, to a cash hash
    fn cache_map(&self) -> PathBuf {
        let p = self.root.join("cache").join("map");
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn insert_cache_map(&self, cache_hash: &str, state_hash: &str) {
        let original = self.cache.the_full_blob_path(cache_hash);
        let link = self.cache_map().join(state_hash);
        if let Err(e) = symlink(&original, &link) {
            dbg!(original);
            dbg!(link);
            dbg!(e);
            //panic!();
        }
    }

    fn try_get_cache_hash(&self, state_hash: &str) -> Option<CacheHash> {
        let path = self.cache_map().join(state_hash);
        if !path.exists() {
            return None;
        };
        Some(
            std::fs::read_link(&path)
                .unwrap()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_string(),
        )
    }

    fn get_cache_hash(&self, state: &str) -> Option<CacheHash> {
        match self.try_get_cache_hash(state) {
            Some(hash) => Some(hash),
            None => {
                let hash = self.rebuild_cache(state)?;
                self.insert_cache_map(&hash, state);
                Some(hash)
            }
        }
    }

    /// lets see lol, there should be a map of statehash to cachehash
    /// if theres not, rebuild entire cachehash from statehash
    ///
    /// each time you run event on the kw store it should update the cache hash
    /// but if you rebuild entire kw store from ledger then updating cache for each gen takes too long
    /// so in htat case it'll just rebuild entire ledger then create a new cache snapshot  from scratch
    /// but in normal mode when one event run at a time it'll update the cachestate incrementally
    pub fn get_the_cache(
        &self,
        state_hash: &str,
        cache_key: &CacheKey<T::PropertyType, T::RefType>,
    ) -> Vec<Key> {
        let Some(cache_hash) = self.get_cache_hash(state_hash) else {
            return vec![];
        };

        self.cache.get_cache(&cache_hash, cache_key)
    }

    fn cachegetter(&self, hash: Option<String>) -> FixedLedger<T> {
        let selv = self.clone();
        FixedLedger {
            inner: Either::Left(selv),
            hash: hash.into(),
        }
    }

    fn rebuild_the_cache(
        &self,
        items: HashMap<T::Key, T>,
        fixed: FixedLedger<T>,
    ) -> Option<CacheHash>
    where
        T: Send + Sync, // Add this bound if not already present
        T::PropertyType: Send + Sync + 'static,
        T::RefType: Send + Sync + 'static,
    {
        info!("rebuilding cache maan");

        use rayon::prelude::*;

        let caches: HashSet<(CacheKey<T::PropertyType, T::RefType>, String)> = items
            .into_par_iter() // Parallel iterator
            .map(|(_, item)| item.caches(&fixed)) // Returns an iterator
            .flatten() // Flattens all those iterators
            .collect(); // Collects into a HashSet

        let cache_map: HashMap<CacheKey<T::PropertyType, T::RefType>, Vec<String>> = {
            let mut cache_map: HashMap<CacheKey<T::PropertyType, T::RefType>, Vec<String>> =
                Default::default();
            for (idx, (key, item)) in caches.into_iter().enumerate() {
                if idx % 1000 == 0 {
                    info!("inserted {}/{}", idx, cache_map.len());
                }
                cache_map.entry(key).or_default().push(item);
            }
            cache_map
        };

        let mut stringied_keys: HashSet<String> = Default::default();

        for key in cache_map.clone() {
            let stringified = key.0.to_string();
            if !stringied_keys.insert(stringified.clone()) {
                //
                panic!();
            }
        }

        let mut stringied_keys: HashMap<String, CacheKey<T::PropertyType, T::RefType>> =
            Default::default();

        for key in cache_map.clone() {
            let stringified = key.0.to_string();
            if let Some(_) = stringied_keys.insert(stringified.clone(), key.0.clone()) {
                panic!();
            }
        }

        let map_len = cache_map.len();

        let mut cache_hash: Option<String> = None;

        for (idx, (key, item)) in cache_map.clone().into_iter().enumerate() {
            if idx % 1000 == 0 && idx != 0 {
                info!("hey inserted {}/{}", idx, map_len);
            }

            if let Some(cachehash) = cache_hash.as_ref() {
                let before = self.cache.get_cache(cachehash, &key);
                if !before.is_empty() {

                    //
                }
            }

            let mut inserted: Vec<String> = item.to_vec();
            let (top, _leaf) =
                self.cache
                    .save_cache(cache_hash.as_deref(), &key, item, &mut vec![]);
            cache_hash = Some(top);
            let mut retrieved = self
                .cache
                .get_cache(cache_hash.clone().unwrap().as_str(), &key);
            inserted.sort();
            retrieved.sort();

            if inserted != retrieved {
                //
            }
        }

        cache_hash
    }

    /// This will go through the entire state and create a hash for it
    fn rebuild_cache(&self, state_hash: &str) -> Option<CacheHash> {
        let items = self.load_all_on_state(state_hash);
        self.rebuild_the_cache(items, self.cachegetter(Some(state_hash.to_string())))
    }

    fn modify_cache(
        &self,
        prev_state_hash: Option<&str>,
        next_state_hash: &str,
        mut insert: Vec<&(CacheKey<T::PropertyType, T::RefType>, String)>,
        remove: Vec<&(CacheKey<T::PropertyType, T::RefType>, String)>,
    ) {
        info!("modify cache!");

        if insert.is_empty() && remove.is_empty() {
            if let Some(prev_state) = prev_state_hash {
                if let Some(cache_hash) = self.try_get_cache_hash(prev_state) {
                    self.insert_cache_map(&cache_hash, next_state_hash);
                }
            }

            return;
        }

        let mut cache_hash = match prev_state_hash {
            Some(prev_state) => match self.try_get_cache_hash(prev_state) {
                Some(cache_hash) => cache_hash,
                None => {
                    info!("rebuild cache cause not in map");
                    if let Some(cache_hash) = self.rebuild_cache(next_state_hash) {
                        self.insert_cache_map(&cache_hash, next_state_hash);
                    }
                    return;
                }
            },
            None => match insert.pop() {
                Some((key, item)) => {
                    self.cache
                        .save_cache(None, key, vec![item.to_string()], &mut vec![])
                        .0
                }
                None => {
                    panic!();
                }
            },
        };

        info!("starting inserting of caches");
        for (key, item) in insert {
            let (hash, _c) =
                self.cache
                    .save_cache(Some(&cache_hash), key, vec![item.to_string()], &mut vec![]);
            cache_hash = hash;
        }

        info!("starting removing of caches");
        for (key, item) in remove {
            let (hash, _c) = self.cache.remove_cache(&cache_hash, key, item, &mut vec![]);
            cache_hash = hash;
        }

        info!("insertinng cache map entry");
        self.insert_cache_map(&cache_hash, next_state_hash);
    }
}

impl<T: LedgerItem> Ledger<T> {
    pub fn new(root: &Path) -> Self {
        let root = Arc::new(PathBuf::from(root).join(Self::item_name()));
        fs::create_dir_all(&*root).unwrap();

        let snap = SnapFs::new((*root).clone());
        let cache = CacheFs::new((root.join("cache")).clone(), 3);
        let selv = Self {
            ledger: Default::default(),
            snap,
            cache,
            root,
            gc_keep: 100,
            all_paths: Default::default(),
        };

        let ledger = Self::load_ledger(&selv.ledger_path());
        *selv.ledger.write().unwrap() = ledger;
        selv
    }

    fn snapshot_refs(&self) -> PathBuf {
        let p = self.root.join("snaprefs");
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn append_ref(&self, index: usize, c: HashSet<Content>) {
        let mut lines: Vec<&str> = Vec::with_capacity(c.len());
        for c in &c {
            let line = c.as_os_str().to_str().unwrap();
            lines.push(line);
        }
        let p = self.current_reftrack_path(index);
        append_line_to_file(&p, lines).unwrap();
    }

    fn current_reftrack_path(&self, index: usize) -> PathBuf {
        let name = self.current_reftrack_name(index);
        self.snapshot_refs().join(&name)
    }

    fn current_reftrack_name(&self, index: usize) -> String {
        let last_snap_idx = index - (index % self.gc_keep);
        let curr = self.ledger.read().unwrap();

        let hash = curr.get(last_snap_idx).unwrap().data_hash();
        format!("{hash}-{last_snap_idx}")
    }

    fn new_paths_after_snapshot(&self, index: usize) -> HashSet<Content> {
        let mut out: HashSet<Content> = Default::default();

        let path = self.current_reftrack_path(index);
        let Ok(content) = fs::read_to_string(&path) else {
            return out;
        };

        for line in content.lines() {
            let x: &str = line;
            let p = PathBuf::from(x);
            let c = Content::new(p);
            out.insert(c);
        }

        out
    }

    pub fn load_all_on_state(&self, hash: &str) -> HashMap<T::Key, T> {
        let res: HashMap<T::Key, T> = self
            .snap
            .get_all(hash)
            .into_iter()
            .map(|(key, val)| (key, serde_json::from_slice(&val).unwrap()))
            .collect();
        dbg!(res.len());
        dbg!(hash);
        res
    }

    pub fn load_all(&self) -> HashMap<T::Key, T> {
        let Some(hash) = self.state_hash() else {
            return Default::default();
        };

        self.load_all_on_state(&hash)
    }

    pub fn load_ids(&self) -> HashSet<T::Key> {
        let Some(hash) = self.state_hash() else {
            return Default::default();
        };
        self.snap.all_item_ids(&hash)
    }

    pub fn insert_ledger(&self, event: TheLedgerEvent<T>) -> Result<StateHash, EventError<T>> {
        if matches!(&event.action, TheLedgerAction::Delete) {
            match self.load(event.id) {
                Some(_) if !self.get_dependents(event.id).is_empty() => {
                    return Err(EventError::DeletingWithDependencies);
                }
                Some(_) => {}
                None => return Err(EventError::ItemNotFound),
            }
        }

        let old_state_hash = self.state_hash();
        let borrowed: Option<&str> = old_state_hash.as_deref();

        let (state_hash, _) = self.run_event(event.clone(), borrowed, true)?;

        if Some(&state_hash) == old_state_hash.as_ref() {
            tracing::info!("not inserting ledger because it didn't change anything");
            return Ok(state_hash);
        }

        let mut guard = self.ledger.write().unwrap();
        let entry = LedgerEntry::new(guard.last(), event);
        guard.push(entry.clone());

        let ledger_hash = guard.last().unwrap().data_hash();
        entry.save(&self.ledger_path());

        self.save_ledger_state(&ledger_hash, &state_hash);

        Ok(state_hash)
    }

    pub fn state_hash(&self) -> Option<StateHash> {
        trace!("retrieving current state hash");
        let ledger = self.ledger.try_read().unwrap();
        if ledger.is_empty() {
            return None;
        }

        if let Some(ledger_hash) = ledger.last().map(|x| x.data_hash()) {
            trace!("ledger hash: {ledger_hash} for {:?}", &self.root);
        }

        if !self.has_statehash() {
            //return self.mem_rebuild();
        }

        self._state_hash(ledger.as_slice()).unwrap()
    }

    pub fn load_last_applied(&self, id: T::Key) -> Option<T> {
        let (last_applied, _) = self.applied_status(self.ledger.read().unwrap().as_slice());
        match self.snap.get(&last_applied?, id) {
            Some(item) => serde_json::from_slice(&item).unwrap(),
            None => None,
        }
    }

    pub fn load(&self, id: T::Key) -> Option<T> {
        /*
        if let Some(item) = self.item_cache.read().unwrap().get(id) {
            tracing::trace!("cache hit for: {:?}", id);
            return Some(item.clone());
        } else {
            tracing::trace!("cache miss for: {:?}", id);
        }
        */

        trace!("load item from ledger: {id:?}");
        let state = self.state_hash()?;
        trace!(
            "loading item from state: {state} item : {id:?}, root: {:?}",
            &self.root
        );

        match self.snap.get(&state, id) {
            Some(item) => {
                let item: T = serde_json::from_slice(&item).unwrap();
                /*
                self.item_cache
                    .write()
                    .unwrap()
                    .insert(id.clone(), item.clone());
                */
                Some(item)
            }

            None => None,
        }
    }

    pub fn get_dependencies(&self, id: T::Key) -> Vec<T::Key> {
        self.load(id)
            .unwrap()
            .ref_cache()
            .into_values()
            .flatten()
            .collect()
    }

    pub fn get_dependents(&self, id: T::Key) -> Vec<String> {
        let key = CacheKey::Dependents { id: id.to_string() };
        self.get_cache(key)
    }

    pub fn get_prop_cache(&self, key: T::PropertyType, value: String) -> Vec<String> {
        let key = CacheKey::Property {
            property: key,
            value,
        };

        self.get_cache(key)
    }

    pub fn get_ref_cache(&self, key: T::RefType, id: T::Key) -> Vec<String> {
        let key = CacheKey::ItemRef {
            reftype: key,
            id: id.to_string(),
        };

        self.get_cache(key)
    }

    fn get_cache(&self, cache_key: CacheKey<T::PropertyType, T::RefType>) -> Vec<String> {
        let Some(hash) = self.state_hash() else {
            return vec![];
        };

        let cache = self.get_the_cache(&hash, &cache_key);

        cache
    }

    /// Hmm maybe we can have a textfile containg all the paths to keep, like those on every 1000 snapshot, so garbage collection will be esasy
    /// like we can just load that list any time so we won't have to run through all and do
    /// wait no, a textfile for each snapshot, that contains all the new paths that came after that snapshot until next snapshot
    /// so if the gc interval is 100, we the state of snapshot 500 is ABCD, there'd be a file named ABCD500
    /// for each new state after 500, we add all the new paths to it
    /// when we reach 600, we check how many of those paths are in 600, the ones that are not can be safely removed
    fn garbage_collection(&self, index: usize) -> Option<(HashSet<Content>, HashSet<LedgerHash>)> {
        info!("GARBAGE COLLECTION;;");

        let guard = self.ledger.read().unwrap();

        if guard.is_empty() {
            return None;
        }

        /// lets see...
        /// we just need to see if there are _any_ states existing between two snapshots
        /// if there is, we load all the paths from the first snapshot
        /// then we load all the paths from the second snapshot
        /// then we load all the newly added paths between that from the ref thing
        /// we remove all the paths from the newref which don't exist in either the prev or new snapshot
        /// then we remove all the ledgerstate -> hashstate references between the snaps
        #[derive(Clone)]
        struct CleanupStates {
            states: HashSet<LedgerHash>,
        }

        let clean_states: CleanupStates = {
            let mut curr_clean = CleanupStates {
                states: Default::default(),
            };

            for entry in guard.iter() {
                if entry.index < index {
                    continue;
                } else if entry.index == index + self.gc_keep {
                    break;
                } else if self.try_get_state_hash(&entry.data_hash()).is_some() {
                    curr_clean.states.insert(entry.data_hash());
                }
            }

            curr_clean
        };

        let mut to_delete: HashSet<Content> = Default::default();

        if clean_states.states.is_empty() {
            return Default::default();
        }

        let prev_state_hash = self.try_get_state_hash(&guard[index].data_hash())?;
        let next_state_hash = self.try_get_state_hash(&guard[index + self.gc_keep].data_hash())?;

        let old_contents = self.get_all_paths(&prev_state_hash);
        let new_contents = self.get_all_paths(&next_state_hash);
        let added_contents = self.new_paths_after_snapshot(index);

        for content in added_contents {
            if !old_contents.contains(&content) && !new_contents.contains(&content) {
                to_delete.insert(content);
            }
        }

        Some((to_delete, clean_states.states))
    }

    fn get_all_paths(&self, hash: &StateHash) -> Arc<HashSet<Content>> {
        if let Some(c) = self.all_paths.read().unwrap().get(hash) {
            return c.clone();
        }

        let paths = Arc::new(self.snap.all_paths(hash));

        self.all_paths
            .write()
            .unwrap()
            .insert(hash.clone(), paths.clone());
        paths
    }

    fn formatter(name: &str) -> String {
        name.split("::").last().unwrap().to_lowercase()
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

    fn state_map_path(&self) -> PathBuf {
        let p = self.root.join("states");
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn ledger_path(&self) -> PathBuf {
        let p = self.root.join("entries");
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn has_statehash(&self) -> bool {
        let path = self.state_map_path();
        let mut entries = fs::read_dir(path).unwrap();
        entries.next().is_some()
    }

    fn try_get_state_hash(&self, ledger_hash: &str) -> Option<StateHash> {
        let path = self.state_map_path().join(ledger_hash);
        if !path.exists() {
            None
        } else {
            Some(
                fs::read_link(&path)
                    .unwrap()
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string(),
            )
        }
    }

    /*
    fn mem_rebuild(&self) -> Option<StateHash> {
        info!("starting inmemory rebuild");
        panic!();
        let mut snapmem = SnapMem::<T::Key>::default();

        let guard = self.ledger.read().unwrap();

        if guard.is_empty() {
            return None;
        }

        for event in guard.iter() {
            let id = event.event.id;
            let key = id.to_string();

            match &event.event.action {
                TheLedgerAction::Create(_) => todo!(),
                TheLedgerAction::Modify(m) => {
                    let item = snapmem
                        .get(&id.to_string())
                        .map(|x| serde_json::from_slice(x.as_slice()).unwrap())
                        .unwrap_or_else(|| T::new_default(id));
                    //let item = item.run_event(m.clone()).unwrap();
                    let item = serde_json::to_vec(&item).unwrap();
                    snapmem.save(&id.to_string(), item);
                }
                TheLedgerAction::Delete => {
                    snapmem.remove(&key);
                }
            }
        }

        info!("persisting inmem to fs");
        let (state_hash, items) = snapmem.clone().persist(self.snap.clone());
        let items: HashMap<String, T> = items
            .into_iter()
            .map(|(key, val)| (key, serde_json::from_slice(&val).unwrap()))
            .collect();
        let ledger_hash = guard.last().unwrap().data_hash();
        self.save_ledger_state(&ledger_hash, &state_hash);
        info!("inmemory rebuild persisted!!");

        /*
        let fixed = Either::Right(snapmem);
        let fixed = FixedLedger {
            inner: fixed,
            hash: None,
        };
        if let Some(cache_hash) = self.rebuild_the_cache(items, fixed) {
            self.insert_cache_map(&cache_hash, &state_hash);
        }
        */
        Some(state_hash)
    }
    */

    /// Returns last applied entry, and list of entries not applied yet.
    fn applied_status(
        &self,
        ledger: &[LedgerEntry<T>],
    ) -> (Option<StateHash>, Vec<LedgerEntry<T>>) {
        trace!("_state_hash @@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@");
        let mut unapplied_entries: Vec<LedgerEntry<T>> = vec![];

        if ledger.is_empty() {
            trace!("ledger is empty");
            return (None, unapplied_entries);
        }

        let ledger = ledger.iter().rev();

        let mut last_applied = None;

        for entry in ledger {
            let ledger_hash = entry.data_hash();
            if let Some(state_hash) = self.try_get_state_hash(&ledger_hash) {
                last_applied = Some(state_hash);
                break;
            } else {
                unapplied_entries.push(entry.clone());
            }
        }

        return (last_applied, unapplied_entries);
    }

    fn _state_hash(&self, ledger: &[LedgerEntry<T>]) -> Result<Option<StateHash>, EventError<T>> {
        let (mut last_applied, mut unapplied_entries) = self.applied_status(ledger);

        if unapplied_entries.is_empty() {
            return Ok(last_applied);
        }

        //info!("unapplied entries: {unapplied_entries:?}");

        let mut to_delete: HashSet<Content> = Default::default();
        let mut cleanup_states: HashSet<LedgerHash> = Default::default();

        let modify_cache = unapplied_entries.len() < 100;

        info!("start apply unapplied!");
        dbg!(&unapplied_entries);
        dbg!(modify_cache);
        while let Some(entry) = unapplied_entries.pop() {
            let idx = entry.index;
            let (state_hash, new_contents) =
                self.run_event(entry.event.clone(), last_applied.as_deref(), modify_cache)?;
            if modify_cache {
                //self.align_item_cache(entry.event.id);
            }
            self.save_ledger_state(&entry.data_hash(), &state_hash);
            last_applied = Some(state_hash);
            if modify_cache {
                info!("new last applied: {last_applied:?}");
            } else {
                let len = unapplied_entries.len();
                if len % 100 == 0 {
                    info!("remaining unapplied: {len}");
                }
            }

            let new_contents: HashSet<Content> = new_contents.into_iter().collect();

            self.append_ref(idx, new_contents);

            if entry.index % self.gc_keep == 0 && self.gc_keep < entry.index {
                if let Some((content, states)) = self.garbage_collection(entry.index - self.gc_keep)
                {
                    to_delete.extend(content);
                    cleanup_states.extend(states);
                } else {
                    tracing::warn!("failed to do garbage collection for index: {}", entry.index);
                }
            }
        }

        if !modify_cache {
            //self.clear_item_cache();
        }

        let state_root = Arc::new(self.root.join("states"));
        info!("deleting state map entries");
        cleanup_states.par_iter().for_each(|state| {
            let p = state_root.join(state);
            fs::remove_file(&p).unwrap();
        });

        info!("deleting {} blobs", to_delete.len());
        to_delete.into_par_iter().for_each(|c| c.delete().unwrap());

        trace!("current state_hash: {last_applied:?}");
        Ok(last_applied)
    }

    /// Creates a symlink from the hash of a ledger event to its corresponding state
    fn save_ledger_state(&self, ledger_hash: &str, state_hash: &str) -> bool {
        let sp = self.snap.the_full_blob_path(state_hash);
        assert!(sp.exists());
        let ledger_path = self.state_map_path().join(ledger_hash);
        if ledger_path.exists() {
            true
        } else {
            symlink(sp, ledger_path).unwrap();
            false
        }
    }

    fn load_ledger(space: &Path) -> Vec<LedgerEntry<T>> {
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

    /// Clones the current state, modifies it with the new entry, and returns the hash of the new state.
    fn run_event(
        &self,
        event: TheLedgerEvent<T>,
        state_hash: Option<&str>,
        update_cache: bool,
    ) -> Result<HashAndContents, EventError<T>> {
        let mut new_content: Vec<Content> = vec![];
        let prev_state_hash = state_hash;
        if update_cache {
            //info!("running event: {event:?} on hash {state_hash:?}");
        }

        let id = event.id;
        let event = match event.action {
            TheLedgerAction::Create(_) => todo!(),
            TheLedgerAction::Modify(m) => m,
            TheLedgerAction::Delete => {
                info!("deletingg!!");

                let state_hash = state_hash.unwrap();
                let item = self.snap.get(state_hash, id).unwrap();
                let item: T = serde_json::from_slice(&item).unwrap();

                let cachegetter = self.cachegetter(Some(state_hash.to_string()));
                let next_state_hash = self.snap.remove(state_hash, id, &mut new_content);

                let old_cache = if update_cache {
                    item.caches(&cachegetter)
                } else {
                    Default::default()
                };

                // lazy so i just make an empty set to copy the code below lol
                let empty: HashSet<(CacheKey<_, _>, String)> = Default::default();
                let old_cache = old_cache.difference(&empty);
                let old_cache: Vec<&(CacheKey<T::PropertyType, T::RefType>, String)> =
                    old_cache.into_iter().collect();

                if update_cache {
                    self.modify_cache(
                        Some(state_hash),
                        &next_state_hash,
                        Default::default(),
                        old_cache,
                    );
                }

                return Ok((next_state_hash, new_content));
            }
        };

        let mut new_item = true;
        let item = match state_hash {
            Some(hash) => {
                match self
                    .snap
                    .get(hash, id)
                    .map(|v| serde_json::from_slice(&v).unwrap())
                {
                    Some(item) => {
                        new_item = false;
                        item
                    }
                    None => T::new_default(id),
                }
            }
            None => T::new_default(id),
        };

        let hashed = state_hash.map(|x| x.to_owned());

        dbg!(&hashed);
        let cachegetter = self.cachegetter(hashed);

        let old_cache = if !new_item && update_cache {
            item.caches(&cachegetter)
        } else {
            Default::default()
        };

        let id = item.item_id();
        let item = item.run_event(event.clone(), &cachegetter)?;
        let new_caches = if update_cache {
            item.caches(&cachegetter)
        } else {
            Default::default()
        };

        let item = serde_json::to_vec(&item).unwrap();
        let (state_hash, new_contents) = self.snap.save(state_hash, id, item, &mut new_content);

        let added_caches = new_caches.difference(&old_cache);
        let added_caches: Vec<&(CacheKey<T::PropertyType, T::RefType>, String)> =
            added_caches.collect();
        let removed_caches: Vec<&(CacheKey<T::PropertyType, T::RefType>, String)> =
            old_cache.difference(&new_caches).collect();

        if update_cache {
            info!("done running event, new statehash: {state_hash}");
            self.modify_cache(prev_state_hash, &state_hash, added_caches, removed_caches);
        }

        Ok((state_hash, new_contents))
    }
}

/*
impl<'de, T, A> Deserialize<'de> for LedgerEntry<T, A>
where
    A: Hash + Debug,
    T: LedgerItem<A>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct LedgerEntryHelper<T, A> {
            previous: Option<Hashed>,
            index: usize,
            event: TheLedgerEvent<T, A>,
        }

        let helper = LedgerEntryHelper::deserialize(deserializer)?;
        Ok(LedgerEntry {
            previous: helper.previous,
            index: helper.index,
            event: helper.event,
        })
    }
}
*/

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

fn append_line_to_file(path: &std::path::Path, lines: Vec<&str>) -> std::io::Result<()> {
    use std::fs::OpenOptions;
    use std::io::Write;

    let mut file = OpenOptions::new().create(true).append(true).open(path)?;

    for line in lines {
        writeln!(file, "{line}")?;
    }

    Ok(())
}
