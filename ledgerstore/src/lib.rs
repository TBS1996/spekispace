use chrono::{DateTime, Utc};
use either::Either;
use nonempty::NonEmpty;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs::{self, hard_link};
use std::io::{self, Write};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::vec::Vec;
use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    hash::Hash,
    sync::{Arc, RwLock},
};
use tracing::info;
use uuid::Uuid;

mod blockchain;
pub mod ledger_cache;
mod ledger_item;
mod local;
mod read_ledger;
mod remote;
mod staging;
use blockchain::{BlockChain, SetUpstream};
pub mod entry_thing;
mod node;
mod write_ledger;

pub use blockchain::{LedgerAction, LedgerEntry, LedgerEvent};
pub use remote::ChangeSet;

pub use ledger_item::LedgerItem;
pub use read_ledger::ReadLedger;
pub use staging::StagingLedger;
pub use write_ledger::WriteLedger;

pub type CacheKey<T> = Either<PropertyCache<T>, ItemRefCache<T>>;
pub type Blob = Vec<u8>;

/// A wrapper around a directory path on disk.
#[derive(Clone, Debug)]
pub struct DiskDirPath(PathBuf);

impl DiskDirPath {
    /// Creates a new directory at the specified path if it does not already exist.
    pub fn new(path: impl AsRef<Path>) -> io::Result<Self> {
        fs::create_dir_all(&path)?;
        Ok(Self(path.as_ref().to_path_buf()))
    }

    /// Returns Some(DiskDirPath) if the directory exists, otherwise None.
    pub fn open(path: impl AsRef<Path>) -> Option<Self> {
        if path.as_ref().is_dir() {
            Some(Self(path.as_ref().to_path_buf()))
        } else {
            None
        }
    }

    /// Clears the contents of the directory.
    pub fn clear_contents(&self) -> io::Result<()> {
        fs::remove_dir_all(&self.0)?;
        fs::create_dir_all(&self.0)?;
        Ok(())
    }
}

impl Deref for DiskDirPath {
    type Target = PathBuf;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct PropertyCache<T: LedgerItem> {
    pub property: T::PropertyType,
    pub value: String,
}

impl<T: LedgerItem> PropertyCache<T> {
    pub fn new(property: T::PropertyType, value: String) -> Self {
        Self { property, value }
    }
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

/// An expression that evaluates to a set of items.
#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq, PartialOrd, Ord)]
#[serde(bound(deserialize = "T: LedgerItem + DeserializeOwned"))]
pub enum ItemExpr<T: LedgerItem> {
    /// Adds all the items in all the nodes.
    Union(Vec<Self>),
    /// Only the items that are shared in all the nodes.
    Intersection(Vec<Self>),
    /// The items that are in the first node but not in the second.
    Difference(Box<Self>, Box<Self>),
    /// All the items in the state, except the ones in the node.
    Complement(Box<Self>),
    /// All the items in the state. Kinda just an alias for Complement of empty union.
    All,
    /// Just a single item
    Item(T::Key),
    /// All items that share a given property.
    Property {
        /// Which property type you care about
        property: T::PropertyType,
        /// The value that the property should have. e.g. if property is "color", value could be "red".
        value: String,
    },
    /// Set of items based on how they are referenced.
    /// Each item can reference other items based on the T::RefType type.
    /// When one item reference another we say it depends on that item.
    /// The state is a DAG so there's no cycles.
    Reference {
        /// The items whose references we are after
        items: Box<Self>,
        /// The type of dependencies we want to fetch. None means all, as in, get all the dependencies of these items.
        ty: Option<T::RefType>,
        /// If true, fetch items that reference these item(s), meaning, the dependents.
        reversed: bool,
        /// If true, recursively get all the references. No cycles guaranteed as it's a DAG.
        recursive: bool,
        /// Whether to also include the items themselves when evaluating.
        include_self: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash)]
pub struct RefGetter<T: LedgerItem> {
    pub reversed: bool, // whether it fetches links from the item to other items or the way this item being referenced
    pub key: T::Key,    // item in question
    pub ty: Option<T::RefType>, // the way of linking. None means all.
    pub recursive: bool, // recursively get all cards that link
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

/// Various reasons why the state cannot be updated with a certain event.
#[derive(Debug, Clone)]
pub enum EventError<T: LedgerItem> {
    /// State must be a DAG, event introduces a cycle.
    Cycle(Vec<(T::Key, T::RefType)>),
    /// An invariant defined by the user of this library was violated.
    Invariant(T::Error),
    /// Event references an item not found in state.
    ItemNotFound(T::Key),
    /// Cannot delete item because it is referenced by other items.
    DeletingWithDependencies(HashSet<T::Key>),
    /// Remote ledger cannot be modified.
    Remote,
}

pub trait TimeProvider {
    fn current_time(&self) -> std::time::Duration;
}

pub type ProviderId = Uuid;
pub type UnixSeconds = u64;
pub type Hashed = String;
pub type StateHash = Hashed;
pub type LedgerHash = Hashed;
pub type CacheHash = Hashed;

pub enum LedgerType<T: LedgerItem> {
    OverRide(OverrideLedger<T>),
    Normal(Ledger<T>),
}

impl<T: LedgerItem> LedgerType<T> {
    pub fn load(&self, key: T::Key) -> Option<Arc<T>> {
        match self {
            LedgerType::OverRide(ledger) => ledger.load(key),
            LedgerType::Normal(ledger) => ledger.load(key),
        }
    }

    pub fn dependents(&self, key: T::Key) -> HashSet<T::Key> {
        match self {
            LedgerType::OverRide(ledger) => ledger.dependents(key),
            LedgerType::Normal(ledger) => ledger.dependents_recursive(key),
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
    new: HashMap<T::Key, Arc<T>>,
}

impl<T: LedgerItem> OverrideLedger<T> {
    pub fn new(inner: &Ledger<T>, new: T) -> Self {
        let new_id = new.item_id();
        let mut map = HashMap::default();
        map.insert(new_id, Arc::new(new));

        Self {
            inner: inner.clone(),
            new: map,
        }
    }

    pub fn load(&self, key: T::Key) -> Option<Arc<T>> {
        if let Some(val) = self.new.get(&key).cloned() {
            return Some(val);
        } else {
            self.inner.load(key)
        }
    }

    pub fn dependencies(&self, key: T::Key) -> HashSet<T::Key> {
        self.load(key).unwrap().dependencies()
    }

    pub fn dependents(&self, key: T::Key) -> HashSet<T::Key> {
        let mut dependents = self.inner.dependents_recursive(key);

        for (dep_key, val) in self.new.iter() {
            if val.dependencies().contains(&key) {
                dependents.insert(*dep_key);
            }
        }

        dependents
    }
}

use crate::entry_thing::EventNode;
use crate::read_ledger::FsReadLedger;
use crate::remote::Remote;

#[derive(Debug, Clone)]
struct ActionEvalResult<T: LedgerItem> {
    item: CardChange<T>,
    added_caches: HashSet<(CacheKey<T>, T::Key)>,
    removed_caches: HashSet<(CacheKey<T>, T::Key)>,
    is_no_op: bool,
}

#[derive(Debug, Clone)]
struct BatchActionEvalResult<T: LedgerItem> {
    items: Vec<CardChange<T>>,
    added_caches: HashSet<(CacheKey<T>, T::Key)>,
    removed_caches: HashSet<(CacheKey<T>, T::Key)>,
    is_no_op: bool,
}

impl<T: LedgerItem> From<ActionEvalResult<T>> for BatchActionEvalResult<T> {
    fn from(res: ActionEvalResult<T>) -> Self {
        Self {
            items: vec![res.item],
            added_caches: res.added_caches,
            removed_caches: res.removed_caches,
            is_no_op: res.is_no_op,
        }
    }
}

impl<T: LedgerItem> BatchActionEvalResult<T> {
    fn new(res: ActionEvalResult<T>) -> Self {
        Self {
            items: vec![res.item],
            added_caches: res.added_caches,
            removed_caches: res.removed_caches,
            is_no_op: res.is_no_op,
        }
    }

    fn merge(mut self, res: ActionEvalResult<T>) -> Self {
        self.items.push(res.item);
        self.added_caches.extend(res.added_caches);
        self.removed_caches.extend(res.removed_caches);
        self.is_no_op &= res.is_no_op;
        self
    }
}

#[derive(Debug, Clone)]
enum CardChange<T: LedgerItem> {
    Created(Arc<T>),
    Modified(Arc<T>),
    Deleted(T::Key),
    Unchanged(T::Key),
}

impl<T: LedgerItem> CardChange<T> {
    #[allow(dead_code)]
    fn key(&self) -> T::Key {
        match self {
            CardChange::Modified(item) => item.item_id(),
            CardChange::Created(item) => item.item_id(),
            CardChange::Deleted(key) => *key,
            CardChange::Unchanged(key) => *key,
        }
    }
}

use crate::local::Local;

#[derive(Clone, Debug, Hash)]
pub struct Node<T: LedgerItem> {
    id: T::Key,
    deps: Vec<Self>,
}

impl<T: LedgerItem> Node<T> {
    pub fn id(&self) -> T::Key {
        self.id
    }

    pub fn deps(&self) -> &Vec<Self> {
        &self.deps
    }

    pub fn direct_dependencies(&self) -> HashSet<T::Key> {
        self.deps.clone().into_iter().map(|n| n.id()).collect()
    }

    pub fn all_dependencies(&self) -> HashSet<T::Key> {
        let mut out: HashSet<T::Key> = Default::default();

        for dep in &self.deps {
            out.insert(dep.id());
            out.extend(dep.all_dependencies());
        }

        out
    }
}

#[derive(Clone)]
pub struct Ledger<T: LedgerItem> {
    entries: BlockChain<T>,
    ledger_hash: Arc<PathBuf>,
    remote: Arc<Remote<T>>,
    local: Local<T>,
    cache: Arc<RwLock<HashMap<T::Key, Arc<T>>>>,
    full_cache: Arc<AtomicBool>,
}

impl<T: LedgerItem> Ledger<T> {
    pub fn new(root: PathBuf) -> Self {
        let selv = Self::new_no_apply(root);

        let current_hash = selv.entries.current_hash();
        let applied_hash = selv.currently_applied_ledger_hash();

        dbg!(Self::item_name());
        dbg!(&current_hash);
        dbg!(&applied_hash);

        if current_hash != applied_hash {
            selv.apply();
        }

        selv
    }

    pub fn new_no_apply(root: PathBuf) -> Self {
        let root = root.join(Self::item_name());
        let entries = DiskDirPath::new(root.join("entries")).unwrap();
        let entries = BlockChain::new(entries);
        let root = root.join("state");
        let ledger_hash = root.join("applied");

        let remote = Remote::new(&root);
        //let _ = remote.hard_reset_current();

        Self {
            ledger_hash: Arc::new(ledger_hash),
            entries,
            remote: Arc::new(remote),
            cache: Default::default(),
            full_cache: Arc::new(AtomicBool::new(false)),
            local: Local {
                inner: FsReadLedger::new(root),
            },
        }
    }

    pub fn current_commit_date(&self) -> Option<DateTime<Utc>> {
        self.remote.current_commit_date()
    }

    pub fn current_commit(&self) -> Option<String> {
        self.remote.current_commit()
    }

    /// Returns a set where all the keys are sorted so that no item depends on a item to its right.
    pub fn load_set_topologically_sorted(&self, set: ItemExpr<T>) -> Vec<T::Key> {
        let keys = self.load_expr(set);
        self.topological_sort(keys.into_iter().collect())
    }

    /// Sorts all the cards so that no item in the output vector depends on a item to "the right".
    pub fn topological_sort(&self, items: Vec<T::Key>) -> Vec<T::Key> {
        let in_set: HashSet<T::Key> = items.iter().cloned().collect();
        let mut indeg: HashMap<T::Key, usize> = items.iter().cloned().map(|k| (k, 0)).collect();
        let mut adj: HashMap<T::Key, Vec<T::Key>> = HashMap::new();

        // Build graph (dep -> dependents) and indegrees within the induced subgraph.
        for item in &in_set {
            for d in self.dependencies_recursive(*item) {
                if in_set.contains(&d) {
                    adj.entry(d.clone()).or_default().push(item.clone());
                    *indeg.get_mut(item).unwrap() += 1;
                }
            }
        }

        // Stable zero-indegree queue seeded by original order.
        let mut q: VecDeque<T::Key> = items
            .iter()
            .filter(|k| indeg.get(*k) == Some(&0))
            .cloned()
            .collect();

        let mut out = Vec::with_capacity(items.len());
        while let Some(u) = q.pop_front() {
            out.push(u.clone());
            if let Some(dependents) = adj.remove(&u) {
                for v in dependents {
                    let e = indeg.get_mut(&v).unwrap();
                    *e -= 1;
                    if *e == 0 {
                        q.push_back(v);
                    }
                }
            }
        }

        debug_assert_eq!(out.len(), items.len(), "DAG assumed; cycle detected");
        out
    }

    pub fn latest_upstream_commit(
        &self,
        current_version: semver::Version,
        github_user: &str,
        github_repo: &str,
    ) -> Option<String> {
        let upstream = format!("https://github.com/{github_user}/{github_repo}");

        let commit = self.remote.latest_upstream_commit(&upstream)?;
        let remote_min_required_version = dbg!(Remote::<T>::remote_min_version(
            &commit,
            github_user,
            github_repo
        ))?;

        if remote_min_required_version <= current_version {
            Some(commit)
        } else {
            None
        }
    }

    pub fn has_item(&self, key: T::Key) -> bool {
        if self.remote.has_item(key) {
            true
        } else {
            self.local.has_item(key)
        }
    }

    /// Insert new ledgerevent and save the state.
    ///
    /// Full operation is roughly three steps:
    ///
    /// 1. Evaluate event to see what changes it would make to the state.
    /// 2. Apply evaluation result to state.
    /// 3. Save entry in list of entries.
    pub fn modify(&self, event: LedgerEvent<T>) -> Result<(), EventError<T>> {
        self.modify_many(vec![event])
    }

    pub fn modify_many(&self, events: Vec<LedgerEvent<T>>) -> Result<(), EventError<T>> {
        let mut applied_events: Vec<LedgerEvent<T>> = vec![];

        for event in events {
            match event.clone() {
                LedgerEvent::ItemAction { id, action } => {
                    let verify = true;
                    let cache = true;

                    let evaluation = self.evaluate_action(id, action.clone(), verify, cache)?;

                    tracing::debug!("res: {:?}", &evaluation);

                    if !evaluation.is_no_op {
                        self.apply_evaluation(evaluation.clone(), cache).unwrap();
                        applied_events.push(event);
                    }
                }
                LedgerEvent::SetUpstream {
                    commit,
                    upstream_url,
                } => {
                    let set_upstream = SetUpstream {
                        commit,
                        upstream_url,
                    };

                    self.apply_and_save_upstream_commit(set_upstream)?;
                    applied_events.push(event);
                }
                LedgerEvent::DeleteSet { set } => {
                    let dependencies = self.dependents_recursive_set(set.clone());

                    if !dependencies.is_empty() {
                        return Err(EventError::DeletingWithDependencies(dependencies));
                    }

                    // Reversed because we must delete dependencies before the dependents.
                    let mut keys = self
                        .load_set_topologically_sorted(set.into())
                        .into_iter()
                        .rev();

                    let Some(key) = keys.next() else {
                        continue;
                    };

                    let eval_res = self.evaluate_action(key, LedgerAction::Delete, true, true)?;
                    let mut batch = BatchActionEvalResult::new(eval_res);

                    // todo: handle potential bug here, if an error occurs in this loop
                    // we will exit the function having modified state without saving any entries
                    // causing a mismatch between state and entries
                    for key in keys {
                        let res = self.evaluate_action(key, LedgerAction::Delete, true, true)?;
                        batch = batch.merge(res);
                    }

                    if !batch.is_no_op {
                        self.apply_evaluation(batch.clone(), true).unwrap();
                        applied_events.push(event);
                    }
                }
            }
        }

        if applied_events.is_empty() {
            Ok(())
        } else if applied_events.len() == 1 {
            let entry = applied_events.remove(0);
            let entry = EventNode::new_leaf(entry);
            let hashed = self.entries.save_entry(entry);
            self.set_ledger_hash(hashed);
            Ok(())
        } else {
            let entries = NonEmpty::from_vec(applied_events).unwrap();
            let entry = EventNode::new_branch(entries);
            let hash = self.entries.save_entry(entry);
            self.set_ledger_hash(hash);
            Ok(())
        }
    }

    pub fn load_ids(&self) -> HashSet<T::Key> {
        if self.full_cache() {
            return self.cache.read().unwrap().keys().cloned().collect();
        }

        let mut ids = self.local.load_ids();
        ids.extend(self.remote.load_ids());

        ids
    }

    pub fn all_dependents_with_ty(&self, key: T::Key) -> HashSet<(T::RefType, T::Key)> {
        let mut items = self.local.all_dependents_with_ty(key);
        items.extend(self.remote.all_dependents_with_ty(key));

        items
    }

    pub fn has_property(&self, item: T::Key, property: PropertyCache<T>) -> bool {
        if self.remote.has_property(item, property.clone()) {
            true
        } else {
            self.local.has_property(item, property)
        }
    }

    pub fn get_prop_cache(&self, key: PropertyCache<T>) -> HashSet<T::Key> {
        let mut items = self.local.get_property_cache(key.clone());
        items.extend(self.remote.get_property_cache(key));

        items
    }

    fn get_reference_cache(
        &self,
        key: T::Key,
        ty: Option<T::RefType>,
        reversed: bool,
        recursive: bool,
    ) -> HashSet<T::Key> {
        let mut items = self
            .local
            .get_reference_cache(key, ty.clone(), reversed, recursive);
        items.extend(
            self.remote
                .get_reference_cache(key, ty, reversed, recursive),
        );

        items
    }

    /// Same as get_reference_cache but returns (RefType, Key) tuples
    fn get_reference_cache_with_ty(
        &self,
        key: T::Key,
        ty: Option<T::RefType>,
        reversed: bool,
        recursive: bool,
    ) -> HashSet<(T::RefType, T::Key)> {
        let mut items =
            self.local
                .get_reference_cache_with_ty(key, ty.clone(), reversed, recursive);
        items.extend(
            self.remote
                .get_reference_cache_with_ty(key, ty, reversed, recursive),
        );

        items
    }

    pub fn dependencies_recursive_node(&self, key: T::Key) -> Node<T> {
        if self.is_remote(key) {
            self.remote
                .collect_all_dependents_recursive_struct(key, false)
        } else {
            self.local
                .collect_all_dependents_recursive_struct(key, false)
        }
    }

    /// Returns all the dependencies of all the items in the set that are not in the set itself.
    pub fn dependents_recursive_set(&self, set: ItemExpr<T>) -> HashSet<T::Key> {
        let items = self.load_expr(set);
        let mut dependencies: HashSet<T::Key> = HashSet::new();

        for item in &items {
            let item_deps = self.dependents_recursive(*item);
            for dep in item_deps {
                if !items.contains(&dep) {
                    dependencies.insert(dep);
                }
            }
        }

        dependencies
    }

    pub fn dependencies_recursive(&self, key: T::Key) -> HashSet<T::Key> {
        let mut items = self.local.recursive_dependencies(key);
        items.extend(self.remote.recursive_dependencies(key));

        items
    }

    pub fn dependents_direct(&self, key: T::Key) -> HashSet<T::Key> {
        let mut items = self.local.direct_dependents(key);
        items.extend(self.remote.direct_dependents(key));
        items
    }

    pub fn dependents_recursive(&self, key: T::Key) -> HashSet<T::Key> {
        let mut items = self.local.recursive_dependents(key);
        items.extend(self.remote.recursive_dependents(key));
        items
    }

    pub fn load_expr(&self, expr: ItemExpr<T>) -> HashSet<T::Key> {
        let mut items = self.local.load_expr(expr.clone());
        items.extend(self.remote.load_expr(expr));
        items
    }

    fn full_cache(&self) -> bool {
        self.full_cache.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn load_all(&self) -> HashSet<T> {
        if self.full_cache() {
            return self
                .cache
                .read()
                .unwrap()
                .values()
                .map(|v| Arc::unwrap_or_clone(v.clone()))
                .collect();
        }

        let mut items = self.local.load_all();
        items.extend(self.remote.load_all());

        let mut cache_guard = self.cache.write().unwrap();

        for item in items.clone() {
            cache_guard.insert(item.item_id(), Arc::new(item));
        }

        self.full_cache
            .store(true, std::sync::atomic::Ordering::SeqCst);

        items
    }

    pub fn load_with_remote_info(&self, key: T::Key) -> Option<(Arc<T>, bool)> {
        let is_remote = self.is_remote(key);
        let item = self.load(key)?;

        Some((item, is_remote))
    }

    pub fn load_or_default(&self, key: T::Key) -> Arc<T> {
        self.load(key)
            .unwrap_or_else(|| Arc::new(T::new_default(key)))
    }

    pub fn load(&self, key: T::Key) -> Option<Arc<T>> {
        if let Some(item) = self.cache.read().unwrap().get(&key).cloned() {
            return Some(item);
        }

        match self.remote.load(key).or_else(|| self.local.load(key)) {
            Some(item) => {
                let item = Arc::new(item);
                self.cache.write().unwrap().insert(key, item.clone());
                Some(item)
            }
            None => None,
        }
    }

    pub fn currently_applied_ledger_hash(&self) -> Option<LedgerHash> {
        fs::read_to_string(&*self.ledger_hash).ok()
    }

    fn item_path(&self, key: T::Key) -> PathBuf {
        let p = self.remote.item_path(key);
        if p.is_file() {
            return p;
        }

        self.local.item_path(key)
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

    /// Recreates state from list of entries.
    ///
    /// Includes two performance boosting tricks.
    ///
    /// 1. Only verifies state integrity after all the entries have been applied.
    /// 2. Only Saves caches after all entries have been applied.
    ///
    /// This massively speeds up the rebuilding of state, with the caveat that if state is invalid, you can't see exactly which entry(s)
    /// caused it to become invalid. It also means that even if the state is valid after all the entries have been applied, it
    /// could be invalid at some points leading up to the last entry.
    pub fn apply(&self) {
        self.local.inner.clear_state();

        let apply_cache = true; //damn, gotta build caches as we go cause the set actions depend on caches ..

        let mut items: HashMap<T::Key, Arc<T>> = HashMap::default();

        self.remote.checkout_empty().unwrap();

        let mut latest_upstream: Option<SetUpstream> = None;

        for (idx, entry) in self.entries.chain().into_iter() {
            if idx % 50 == 0 {
                dbg!(idx);
            };

            for event in &entry {
                match event.event.clone() {
                    LedgerEvent::ItemAction { id, action } => {
                        let evaluation =
                            match self.evaluate_action(id, action.clone(), false, apply_cache) {
                                Ok(eval) => eval,
                                Err(e) => {
                                    dbg!(e);
                                    dbg!(id, action);
                                    panic!();
                                }
                            };

                        self.apply_evaluation(evaluation.clone(), apply_cache)
                            .unwrap();

                        match evaluation.item {
                            CardChange::Created(item) | CardChange::Modified(item) => {
                                let key = item.item_id();
                                items.insert(key, item);
                            }
                            CardChange::Deleted(key) => {
                                items.remove(&key);
                            }
                            CardChange::Unchanged(_) => {}
                        }
                    }
                    LedgerEvent::SetUpstream {
                        commit,
                        upstream_url,
                    } => {
                        let set_upstream = SetUpstream {
                            commit,
                            upstream_url,
                        };
                        self.set_remote_commit(&set_upstream).unwrap();
                        latest_upstream = Some(set_upstream);
                    }
                    LedgerEvent::DeleteSet { set } => {
                        dbg!(&set);
                        let keys = self.load_set_topologically_sorted(set.into());
                        dbg!(&keys);
                        for key in keys.into_iter().rev() {
                            let eval = self
                                .evaluate_action(key, LedgerAction::Delete, false, apply_cache)
                                .unwrap();
                            self.apply_evaluation(eval, apply_cache).unwrap();
                            items.remove(&key);
                        }
                    }
                };
            }
        }

        if let Some(upstream) = latest_upstream {
            self.set_remote_commit(&upstream).unwrap();
        }

        // if we dont apply cache during build then we need to apply it after
        if !apply_cache {
            self.apply_caches(items);
        }

        self.verify_all().unwrap();

        if let Some(hash) = self.entries.current_hash() {
            info!("{} ledger hash after apply: {}", Self::item_name(), hash);
            self.set_ledger_hash(hash);
        }
    }

    fn apply_caches(&self, items: HashMap<T::Key, Arc<T>>) {
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

    fn set_ledger_hash(&self, hash: LedgerHash) {
        let mut f = fs::File::create(&*self.ledger_hash).unwrap();
        f.write_all(hash.as_bytes()).unwrap();
    }

    fn insert_cache(&self, cache: CacheKey<T>, id: T::Key) {
        match cache {
            CacheKey::Left(PropertyCache { property, value }) => {
                self.insert_property(id, property, value);
            }
            CacheKey::Right(ItemRefCache { reftype, id: to }) => {
                self.insert_reference(ItemReference::new(id, to, reftype));
            }
        }
    }

    fn remove_cache(&self, cache: CacheKey<T>, id: T::Key) {
        match cache {
            CacheKey::Left(PropertyCache { property, value }) => {
                self.remove_property(id, property, value);
            }
            CacheKey::Right(ItemRefCache { reftype, id: to }) => {
                self.remove_reference(ItemReference::new(id, to, reftype));
            }
        }
    }
}

impl<T: LedgerItem> WriteLedger for Ledger<T> {
    type Item = T;

    fn remove(&self, key: T::Key) {
        let path = self.local.item_path(key);
        std::fs::remove_file(path).unwrap();
    }

    fn save(&self, item: T) {
        let key = item.item_id();
        let item_path = self.local.item_path_create(key);

        let serialized = serde_json::to_string_pretty(&item).unwrap();
        let mut f = std::fs::File::create(&item_path).unwrap();
        use std::io::Write;

        f.write_all(serialized.as_bytes()).unwrap();
        self.set_dependencies(&item);
    }

    fn set_dependencies(&self, item: &T) {
        let id = item.item_id();
        let dependencies_dir = self.local.inner.root_dependencies_dir(id);

        if dependencies_dir.exists() {
            fs::remove_dir_all(&dependencies_dir).unwrap();
        }

        let ref_caches = item.ref_cache();

        let dependencies_dir = if !ref_caches.is_empty() {
            self.local.inner.root_dependencies_dir(id) //recreate it
        } else {
            return;
        };

        for ItemReference { from: _, to, ty } in ref_caches {
            let dir = dependencies_dir.join(ty.to_string());
            fs::create_dir_all(&dir).unwrap();
            let original = self.item_path(to); // Uses Ledger's item_path for remote+local
            let link = dir.join(to.to_string());
            if let Err(e) = hard_link(&original, &link) {
                dbg!(e, original, link);
                panic!();
            }
        }
    }

    fn remove_property(&self, key: T::Key, property: T::PropertyType, value: String) {
        let path = self
            .local
            .inner
            .properties_path()
            .join(property.to_string())
            .join(&value)
            .join(key.to_string());
        let _ = fs::remove_file(&path);
    }

    fn insert_property(&self, key: T::Key, property: T::PropertyType, value: String) {
        let dir = self
            .local
            .inner
            .properties_path()
            .join(property.to_string())
            .join(&value);
        fs::create_dir_all(&dir).unwrap();
        let original = self.item_path(key); // Uses Ledger's item_path for remote+local
        let link = dir.join(key.to_string());
        hard_link(original, link).unwrap();
    }

    fn insert_reference(&self, reference: ItemReference<T>) {
        // Creates a link in dependents/to/ty/from
        // ItemReference { from, to, ty } means 'from' item references 'to' item via 'ty'
        // So we track this in dependents/to/ty/from
        let ItemReference { from, to, ty } = reference;
        let dir = self
            .local
            .inner
            .root_dependents_dir(to)
            .join(ty.to_string());
        fs::create_dir_all(&dir).unwrap();
        let original = self.item_path(from); // Uses Ledger's item_path for remote+local
        let link = dir.join(from.to_string());
        hard_link(original, link).unwrap();
    }

    fn remove_reference(&self, reference: ItemReference<T>) {
        let ItemReference { from, to, ty } = reference;
        let path = self
            .local
            .inner
            .root_dependents_dir(to)
            .join(ty.to_string())
            .join(from.to_string());
        let _ = fs::remove_file(&path);
    }
}

impl<T: LedgerItem> Ledger<T> {
    fn is_remote(&self, key: T::Key) -> bool {
        self.remote.has_item(key)
    }

    fn set_remote_commit(&self, set_upstream: &SetUpstream) -> Result<(), EventError<T>> {
        let current_commit = self.remote.current_commit();
        let ChangeSet {
            added: _,
            modified,
            removed,
        } = self
            .remote
            .set_commit_clean(Some(&set_upstream.upstream_url), &set_upstream.commit)
            .unwrap();

        for item in removed {
            if !self.local.direct_dependents(item).is_empty() {
                match current_commit {
                    Some(commit) => {
                        let _ = self.remote.set_commit_clean(None, &commit).unwrap();
                    }
                    None => {
                        self.remote.reset_to_empty();
                    }
                }

                return Err(EventError::ItemNotFound(item));
            }
        }

        let mut dependents: HashSet<T::Key> = HashSet::default();

        for item in &modified {
            dependents.extend(self.local.direct_dependents(*item));
        }

        //dbg!(&modified, &dependents);

        for dependent in dependents {
            let item = self.local.load(dependent).unwrap();

            let _ = match item.verify(self) {
                Ok(item) => item,
                Err(e) => {
                    match current_commit {
                        Some(commit) => {
                            let _ = self.remote.set_commit_clean(None, &commit).unwrap();
                        }
                        None => {
                            self.remote.reset_to_empty();
                        }
                    }
                    return Err(e);
                }
            };
        }

        Ok(())
    }

    /// See how a [`LedgerAction`] would change the state, without actually saving the result.
    fn evaluate_action(
        &self,
        key: T::Key,
        action: LedgerAction<T>,
        verify: bool, // if true, check if action will uphold invariants
        cache: bool,  // if true, return cache modification results.
    ) -> Result<ActionEvalResult<T>, EventError<T>> {
        if self.is_remote(key) && verify {
            return Err(EventError::Remote);
        }

        let (old_caches, new_caches, item, is_no_op) = match action.clone() {
            LedgerAction::Modify(action) => {
                let (old_caches, old_item) = match self.load(key) {
                    Some(item) if cache => (item.caches(self), item),
                    Some(item) => (Default::default(), item),
                    None => (Default::default(), Arc::new(T::new_default(key))),
                };
                let old_cloned = old_item.clone();
                let modified_item =
                    Arc::new(Arc::unwrap_or_clone(old_item).run_event(action, self, verify)?);

                let no_op = old_cloned == modified_item;

                let item = if no_op {
                    CardChange::Unchanged(modified_item.item_id())
                } else {
                    CardChange::Modified(modified_item.clone())
                };

                let new_caches = modified_item.caches(self);
                (old_caches, new_caches, item, no_op)
            }
            LedgerAction::Create(mut item) => {
                if verify {
                    item = item.verify(self)?;
                }
                let caches = if cache {
                    item.caches(self)
                } else {
                    Default::default()
                };

                let item = Arc::new(item);

                (HashSet::default(), caches, CardChange::Created(item), false)
            }
            LedgerAction::Delete => {
                let old_item = self.load(key).unwrap();
                let old_caches = old_item.caches(self);
                (
                    old_caches,
                    Default::default(),
                    CardChange::Deleted(key),
                    false,
                )
            }
        };

        let added_caches = &new_caches - &old_caches;
        let removed_caches = &old_caches - &new_caches;

        Ok(ActionEvalResult {
            item,
            added_caches,
            removed_caches,
            is_no_op,
        })
    }

    fn apply_evaluation(
        &self,
        res: impl Into<BatchActionEvalResult<T>>,
        cache: bool,
    ) -> Result<(), EventError<T>> {
        let BatchActionEvalResult {
            items,
            added_caches,
            removed_caches,
            is_no_op,
        } = res.into();

        if is_no_op {
            if !added_caches.is_empty() || !removed_caches.is_empty() {
                //dbg!(&item, &added_caches, &removed_caches);
            }
        }

        for item in items {
            match item {
                CardChange::Created(item) | CardChange::Modified(item) => {
                    self.cache
                        .write()
                        .unwrap()
                        .insert(item.item_id(), item.clone());

                    self.save(Arc::unwrap_or_clone(item));
                }
                CardChange::Deleted(key) => {
                    self.cache.write().unwrap().remove(&key);
                    debug_assert!(added_caches.is_empty());
                    self.remove(key);
                    dbg!(key);
                }
                CardChange::Unchanged(_) => {}
            }
        }

        if !cache {
            return Ok(());
        }

        for (cache, key) in added_caches {
            self.insert_cache(cache, key);
        }

        for (cache, key) in removed_caches {
            self.remove_cache(cache, key);
        }

        Ok(())
    }

    fn apply_and_save_upstream_commit(&self, event: SetUpstream) -> Result<(), EventError<T>> {
        self.set_remote_commit(&event)?;
        self.save_event(event);
        return Ok(());
    }

    fn save_event(&self, event: impl Into<LedgerEvent<T>>) {
        let entry = EventNode::new_leaf(event.into());
        let hash = self.entries.save_entry(entry);
        self.set_ledger_hash(hash);
    }
}
