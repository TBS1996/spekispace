use rayon::prelude::*;
use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned};
use simpletime::timed;
use snapstore::fs::{CacheFs, Content, SnapFs};
use snapstore::{Key, SnapStorage};
use std::fmt::Display;
use std::fs::{self};
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
use tracing::{info, trace};
use uuid::Uuid;

pub mod ledger_cache;

pub use snapstore::CacheKey;

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

//pub struct Ledger<T: LedgerItem<E, S>, E: LedgerEvent, S: Clone = ()> {

/// Represents how a ledger mutates or creates an item.
pub trait LedgerItem<E: LedgerEvent + Debug>:
    Serialize + DeserializeOwned + Hash + Clone + 'static
{
    type Error: Debug;
    type RefType: AsRef<str> + Display + Clone + Hash + PartialEq + Eq;
    type PropertyType: AsRef<str> + Display + Clone + Hash + PartialEq + Eq;

    fn run_event(self, event: E) -> Result<Self, Self::Error>;

    fn new_default(id: E::Key) -> Self;

    fn item_id(&self) -> E::Key;

    /// List of references to other items, along with the name of the type of reference.
    ///
    /// Used to create a index, like if item A references item B, we cache that item B is referenced by item A,
    /// so that we don't need to search through all the items to find out or store it double in the item itself.
    fn ref_cache(&self) -> HashMap<Self::RefType, HashSet<E::Key>> {
        Default::default()
    }

    fn dependencies(&self) -> HashSet<E::Key> {
        self.ref_cache().into_values().flatten().collect()
    }

    /// List of defined properties that this item has.
    ///
    /// The property keys are predefined, hence theyre static str
    /// the String is the Value which could be anything.
    /// For example ("suspended", true).
    fn properties_cache(
        &self,
        ledger: FixedLedger<Self, E>,
    ) -> HashSet<(Self::PropertyType, String)>
    where
        Self: LedgerItem<E>,
    {
        let _ = ledger;
        Default::default()
    }

    fn caches(
        &self,
        ledger: FixedLedger<Self, E>,
    ) -> HashSet<(CacheKey<Self::PropertyType, Self::RefType>, String)>
    where
        Self: LedgerItem<E>,
    {
        trace!("fetching caches for item: {:?}", self.item_id());

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
pub struct FixedLedger<T: LedgerItem<E>, E: LedgerEvent> {
    inner: Ledger<T, E>,
    hash: Option<StateHash>,
}

impl<T: LedgerItem<E>, E: LedgerEvent> FixedLedger<T, E> {
    pub fn load(&self, id: E::Key) -> Option<T> {
        let state = self.hash.as_ref()?;
        match self.inner.snap.get(state, id.to_string().as_str()) {
            Some(item) => serde_json::from_slice(&item).unwrap(),
            None => None,
        }
    }
}

#[derive(Clone)]
pub struct Ledger<T: LedgerItem<E>, E: LedgerEvent> {
    ledger: Arc<RwLock<Vec<LedgerEntry<E>>>>,
    snap: SnapFs,
    cache: CacheFs<T::PropertyType, T::RefType>,
    root: Arc<PathBuf>,
    _phantom: PhantomData<T>,
    gc_keep: usize,
    all_paths: Arc<RwLock<HashMap<StateHash, Arc<HashSet<Content>>>>>,
}

impl<T: LedgerItem<E>, E: LedgerEvent> Ledger<T, E> {
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
    fn get_the_cache(
        &self,
        state_hash: &str,
        cache_key: &CacheKey<T::PropertyType, T::RefType>,
    ) -> Vec<Key> {
        let Some(cache_hash) = self.get_cache_hash(state_hash) else {
            return vec![];
        };

        self.cache.get_cache(&cache_hash, cache_key)
    }

    fn cachegetter(&self, hash: impl Into<Option<String>>) -> FixedLedger<T, E> {
        FixedLedger {
            inner: (*self).clone(),
            hash: hash.into(),
        }
    }

    /// This will go through the entire state and create a hash for it
    fn rebuild_cache(&self, state_hash: &str) -> Option<CacheHash> {
        info!("rebuilding cache");
        let caches: HashSet<(CacheKey<T::PropertyType, T::RefType>, String)> = self
            .load_all_on_state(state_hash)
            .into_values()
            .flat_map(|item| item.caches(self.cachegetter(state_hash.to_owned())))
            .collect();

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
            if let Some(old_key) = stringied_keys.insert(stringified.clone(), key.0.clone()) {
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
            let (top, _leaf) = self.cache.save_cache(cache_hash.as_deref(), &key, item);
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

            dbg!("not updating cache!!");
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
                Some((key, item)) => self.cache.save_cache(None, key, vec![item.to_string()]).0,
                None => {
                    panic!();
                }
            },
        };

        info!("starting inserting of caches");
        for (key, item) in insert {
            let (hash, _c) = self
                .cache
                .save_cache(Some(&cache_hash), key, vec![item.to_string()]);
            cache_hash = hash;
        }

        info!("starting removing of caches");
        for (key, item) in remove {
            let (hash, _c) = self.cache.remove_cache(&cache_hash, key, item);
            cache_hash = hash;
        }

        info!("insertinng cache map entry");
        self.insert_cache_map(&cache_hash, next_state_hash);
    }
}

impl<T: LedgerItem<E>, E: LedgerEvent> Ledger<T, E> {
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
            _phantom: PhantomData,
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

        let hash = curr.get(last_snap_idx).unwrap().hash();
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

    pub fn load_all_on_state(&self, hash: &str) -> HashMap<String, T> {
        self.snap
            .get_all(hash)
            .into_iter()
            .map(|(key, val)| (key, serde_json::from_slice(&val).unwrap()))
            .collect()
    }

    pub fn load_all(&self) -> HashMap<String, T> {
        let Some(hash) = self.state_hash() else {
            return Default::default();
        };

        self.load_all_on_state(&hash)
    }

    pub fn load_ids(&self) -> Vec<String> {
        let Some(hash) = self.state_hash() else {
            return Default::default();
        };
        self.snap.get_all(&hash).into_keys().collect()
    }

    pub fn insert_ledger(&self, event: E) {
        let mut guard = self.ledger.write().unwrap();
        let entry = LedgerEntry::new(guard.last(), event);
        guard.push(entry.clone());
        let name = format!("{:06}", entry.index);
        let path = self.ledger_path().join(name);
        let mut file = std::fs::File::create_new(&path).unwrap();
        file.write_all(serde_json::to_string_pretty(&entry).unwrap().as_bytes())
            .unwrap();
    }

    pub fn state_hash(&self) -> Option<StateHash> {
        trace!("retrieving current state hash");
        let ledger = self.ledger.try_read().unwrap();

        self._state_hash(ledger.as_slice())
    }

    pub fn load_last_applied(&self, id: &str) -> Option<T> {
        let (last_applied, _) = self.applied_status(self.ledger.read().unwrap().as_slice());
        match self.snap.get(&last_applied?, id) {
            Some(item) => serde_json::from_slice(&item).unwrap(),
            None => None,
        }
    }

    pub fn load(&self, id: impl AsRef<E::Key>) -> Option<T> {
        let id = id.as_ref();
        trace!("load item from ledger: {id:?}");
        let state = self.state_hash()?;
        trace!("loading item from state: {state} item : {id:?}");
        match self.snap.get(&state, id.to_string().as_str()) {
            Some(item) => serde_json::from_slice(&item).unwrap(),
            None => None,
        }
    }

    pub fn get_dependencies(&self, id: impl AsRef<E::Key>) -> Vec<E::Key> {
        self.load(id)
            .unwrap()
            .ref_cache()
            .into_values()
            .flatten()
            .collect()
    }

    pub fn get_dependents(&self, id: E::Key) -> Vec<String> {
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

    pub fn get_ref_cache(&self, key: T::RefType, id: E::Key) -> Vec<String> {
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
    fn garbage_collection(&self, index: usize) -> (HashSet<Content>, HashSet<LedgerHash>) {
        info!("GARBAGE COLLECTION;;");

        let guard = self.ledger.read().unwrap();

        if guard.is_empty() {
            return Default::default();
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
                } else if self.try_get_state_hash(&entry.hash()).is_some() {
                    curr_clean.states.insert(entry.hash());
                }
            }

            curr_clean
        };

        let mut to_delete: HashSet<Content> = Default::default();

        if clean_states.states.is_empty() {
            return Default::default();
        }

        let prev_state_hash = self.try_get_state_hash(&guard[index].hash()).unwrap();
        let next_state_hash = self
            .try_get_state_hash(&guard[index + self.gc_keep].hash())
            .unwrap();

        let old_contents = self.get_all_paths(&prev_state_hash);
        let new_contents = self.get_all_paths(&next_state_hash);
        let added_contents = self.new_paths_after_snapshot(index);

        for content in added_contents {
            if !old_contents.contains(&content) && !new_contents.contains(&content) {
                to_delete.insert(content);
            }
        }

        (to_delete, clean_states.states)
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

    /// Returns last applied entry, and list of entries not applied yet.
    fn applied_status(
        &self,
        ledger: &[LedgerEntry<E>],
    ) -> (Option<StateHash>, Vec<LedgerEntry<E>>) {
        trace!("_state_hash @@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@");
        let mut unapplied_entries: Vec<LedgerEntry<E>> = vec![];

        if ledger.is_empty() {
            trace!("ledger is empty");
            return (None, unapplied_entries);
        }

        let ledger = ledger.iter().rev();

        let mut last_applied = None;

        for entry in ledger {
            let ledger_hash = entry.hash();
            if let Some(state_hash) = self.try_get_state_hash(&ledger_hash) {
                last_applied = Some(state_hash);
                break;
            } else {
                unapplied_entries.push(entry.clone());
            }
        }

        return (last_applied, unapplied_entries);
    }

    fn _state_hash(&self, ledger: &[LedgerEntry<E>]) -> Option<StateHash> {
        let (mut last_applied, mut unapplied_entries) = self.applied_status(ledger);

        if unapplied_entries.is_empty() {
            return last_applied;
        }

        info!("unapplied entries: {unapplied_entries:?}");

        let mut to_delete: HashSet<Content> = Default::default();
        let mut cleanup_states: HashSet<LedgerHash> = Default::default();

        let modify_cache = unapplied_entries.len() < 100;

        info!("start apply unapplied!");
        while let Some(entry) = unapplied_entries.pop() {
            let idx = entry.index;
            let (state_hash, new_contents) =
                timed!(self.run_event(entry.event.clone(), last_applied.as_deref(), modify_cache));
            timed!(self.save_ledger_state(&entry.hash(), &state_hash));
            last_applied = Some(state_hash);
            info!("new last applied: {last_applied:?}");

            let new_contents: HashSet<Content> = new_contents.into_iter().collect();

            timed!(self.append_ref(idx, new_contents));

            if entry.index % self.gc_keep == 0 && self.gc_keep < entry.index {
                let (content, states) = self.garbage_collection(entry.index - self.gc_keep);
                to_delete.extend(content);
                cleanup_states.extend(states);
            }
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
        last_applied
    }

    /// Creates a symlink from the hash of a ledger event to its corresponding state
    fn save_ledger_state(&self, ledger_hash: &str, state_hash: &str) {
        let sp = self.snap.the_full_blob_path(state_hash);
        assert!(sp.exists());
        let ledger_path = self.state_map_path().join(ledger_hash);
        symlink(sp, ledger_path).unwrap();
    }

    fn load_ledger(space: &Path) -> Vec<LedgerEntry<E>> {
        let mut foo: Vec<(usize, LedgerEntry<E>)> = {
            let map: HashMap<String, Vec<u8>> = load_file_contents(space);
            let mut foo: Vec<(usize, LedgerEntry<E>)> = Default::default();

            if map.is_empty() {
                return vec![];
            }

            for (_hash, value) in map.into_iter() {
                let action: LedgerEntry<E> = serde_json::from_slice(&value).unwrap();
                let idx = action.index;
                foo.push((idx, action));
            }

            foo
        };

        foo.sort_by_key(|k| k.0);

        let mut output: Vec<LedgerEntry<E>> = vec![];
        let mut prev_hash: Option<String> = None;

        for (_, entry) in foo {
            //assert_eq!(entry.previous.clone(), prev_hash);
            prev_hash = Some(entry.hash());
            output.push(entry);
        }

        output
    }

    /// Clones the current state, modifies it with the new entry, and returns the hash of the new state.
    fn run_event(
        &self,
        event: E,
        state_hash: Option<&str>,
        update_cache: bool,
    ) -> (StateHash, Vec<Content>) {
        let prev_state_hash = state_hash;
        info!("running event: {event:?} on hash {state_hash:?}");

        let mut new_item = true;
        let item = match state_hash {
            Some(hash) => {
                match self
                    .snap
                    .get(hash, &event.id().to_string())
                    .map(|v| serde_json::from_slice(&v).unwrap())
                {
                    Some(item) => {
                        new_item = false;
                        item
                    }
                    None => T::new_default(event.id()),
                }
            }
            None => T::new_default(event.id()),
        };

        let cachegetter = self.cachegetter(state_hash.map(ToOwned::to_owned));

        let old_cache = if !new_item && update_cache {
            timed!(item.caches(cachegetter.clone()))
        } else {
            Default::default()
        };

        let id = item.item_id();
        let item = timed!(item.run_event(event.clone()).unwrap());
        let new_caches = if update_cache {
            item.caches(cachegetter)
        } else {
            Default::default()
        };

        let item = serde_json::to_vec(&item).unwrap();
        let (state_hash, new_contents) = timed!(self.snap.save(state_hash, &id.to_string(), item));

        let added_caches = new_caches.difference(&old_cache);
        let added_caches: Vec<&(CacheKey<T::PropertyType, T::RefType>, String)> =
            added_caches.collect();
        let removed_caches: Vec<&(CacheKey<T::PropertyType, T::RefType>, String)> =
            old_cache.difference(&new_caches).collect();

        info!("done running event, new statehash: {state_hash}");

        if update_cache {
            self.modify_cache(prev_state_hash, &state_hash, added_caches, removed_caches);
        }

        (state_hash, new_contents)
    }
}

#[derive(Clone, Serialize, Debug)]
struct LedgerEntry<E: LedgerEvent> {
    previous: Option<Hashed>,
    index: usize,
    event: E,
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

impl<E: LedgerEvent + Serialize + DeserializeOwned> LedgerEntry<E> {
    fn new(previous: Option<&Self>, event: E) -> Self {
        let (index, previous) = match previous {
            Some(e) => (e.index + 1, Some(e.hash())),
            None => (0, None),
        };
        Self {
            previous,
            index,
            event,
        }
    }

    fn hash(&self) -> Hashed {
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

fn append_line_to_file(path: &std::path::Path, lines: Vec<&str>) -> std::io::Result<()> {
    use std::fs::OpenOptions;
    use std::io::Write;

    let mut file = OpenOptions::new().create(true).append(true).open(path)?;

    for line in lines {
        writeln!(file, "{line}")?;
    }

    Ok(())
}
