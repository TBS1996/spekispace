use chrono::{DateTime, Utc};
use either::Either;
use indexmap::IndexSet;
use nonempty::NonEmpty;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::{BTreeSet, VecDeque};
use std::fs::{self};
use std::io::{self, Write};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::vec::Vec;
use std::{
    collections::HashMap,
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
mod item;
mod node;
mod write_ledger;

pub use blockchain::{ItemAction, LedgerAction, LedgerEntry, LedgerEvent};
pub use remote::ChangeSet;

pub use item::SavedItem;
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

/// Represents a reference to another item.
/// Does not contain information on which item contains this reference.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct ItemRefCache<T: LedgerItem> {
    pub reftype: T::RefType,
    /// The target item being referenced (the dependency)
    pub referent: T::Key,
}

impl<T: LedgerItem> ItemRefCache<T> {
    pub fn new(reftype: T::RefType, referent: T::Key) -> Self {
        Self { reftype, referent }
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
    DeletingWithDependencies(IndexSet<T::Key>),
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

use crate::entry_thing::EventNode;
use crate::read_ledger::FsReadLedger;
use crate::remote::Remote;

#[derive(Debug, Clone)]
struct ActionEvalResult<T: LedgerItem> {
    item: CardChange<T>,
    added_caches: IndexSet<(CacheKey<T>, T::Key)>,
    removed_caches: IndexSet<(CacheKey<T>, T::Key)>,
    is_no_op: bool,
}

#[derive(Debug, Clone)]
struct BatchActionEvalResult<T: LedgerItem> {
    items: Vec<CardChange<T>>,
    added_caches: IndexSet<(CacheKey<T>, T::Key)>,
    removed_caches: IndexSet<(CacheKey<T>, T::Key)>,
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
pub enum CardChange<T: LedgerItem> {
    Created(Arc<T>),
    Modified(Arc<T>),
    Deleted(T::Key),
    Unchanged(T::Key),
}

impl<T: LedgerItem> CardChange<T> {
    pub fn key(&self) -> T::Key {
        match self {
            CardChange::Modified(item) => item.item_id(),
            CardChange::Created(item) => item.item_id(),
            CardChange::Deleted(key) => *key,
            CardChange::Unchanged(key) => *key,
        }
    }

    pub fn print_terse(&self) -> String {
        let id = self.key().to_string();
        match self {
            CardChange::Created(_) => format!("Created {}", id),
            CardChange::Modified(_) => format!("Modified {}", id),
            CardChange::Deleted(_) => format!("Deleted {}", id),
            CardChange::Unchanged(_) => format!("Unchanged {}", id),
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

    pub fn direct_dependencies(&self) -> IndexSet<T::Key> {
        self.deps.clone().into_iter().map(|n| n.id()).collect()
    }

    pub fn all_dependencies(&self) -> IndexSet<T::Key> {
        let mut out: IndexSet<T::Key> = Default::default();

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
    cache: Arc<RwLock<HashMap<T::Key, SavedItem<T>>>>,
    full_cache: Arc<AtomicBool>,
    recent_items_path: Arc<PathBuf>,
}

impl<T: LedgerItem> Ledger<T> {
    pub fn new(root: PathBuf) -> Self {
        let selv = Self::new_no_apply(root);

        let current_hash = selv.entries.current_hash();
        let applied_hash = selv.currently_applied_ledger_hash();

        if current_hash != applied_hash {
            selv.apply();
        }

        selv
    }

    pub fn new_staging(&self) -> StagingLedger<T> {
        StagingLedger::new(self.clone())
    }

    pub fn new_no_apply(root: PathBuf) -> Self {
        let root = root.join(Self::item_name());
        let entries = DiskDirPath::new(root.join("entries")).unwrap();
        let entries = BlockChain::new(entries);
        let root = root.join("state");
        let ledger_hash = root.join("applied");
        let recent_items_path = root.join("recent_items");

        let remote = Remote::new(&root);
        //let _ = remote.hard_reset_current();

        Self {
            ledger_hash: Arc::new(ledger_hash),
            entries,
            remote: Arc::new(remote),
            cache: Default::default(),
            full_cache: Arc::new(AtomicBool::new(false)),
            recent_items_path: Arc::new(recent_items_path),
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

    /// Returns the current upstream URL from the most recent SetUpstream event.
    /// The remote is fetched anonymously and not stored as a git remote.
    pub fn current_upstream_url(&self) -> Option<String> {
        for (_idx, entry) in self.entries.chain().into_iter().rev() {
            for event in &entry {
                if let LedgerEvent::SetUpstream { upstream_url, .. } = &event.event {
                    return Some(upstream_url.clone());
                }
            }
        }
        None
    }

    /// Returns a set where all the keys are sorted so that no item depends on a item to its right.
    pub fn load_set_topologically_sorted(&self, set: ItemExpr<T>) -> Vec<T::Key> {
        let keys = self.load_expr(set);
        self.topological_sort(keys.into_iter().collect())
    }

    /// Sorts all the cards so that no item in the output vector depends on a item to "the right".
    pub fn topological_sort(&self, items: Vec<T::Key>) -> Vec<T::Key> {
        let in_set: IndexSet<T::Key> = items.iter().cloned().collect();
        let mut indeg: HashMap<T::Key, usize> = items.iter().cloned().map(|k| (k, 0)).collect();
        let mut adj: HashMap<T::Key, Vec<T::Key>> = HashMap::new();

        // Build graph (dep -> dependents) and indegrees within the induced subgraph.
        for item in &in_set {
            for d in self.dependencies_recursive(*item) {
                // Skip self-loops - items are allowed to depend on themselves
                if d == *item {
                    continue;
                }
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
        match event.clone() {
            LedgerEvent::ItemAction { id, action } => {
                let evaluation = self.evaluate_action(id, action.clone(), true, true)?;

                tracing::debug!("res: {:?}", &evaluation);

                if !evaluation.is_no_op {
                    self.apply_evaluation(evaluation.clone(), true).unwrap();
                    self.save_event(event);

                    // Track recently modified item and its direct dependencies
                    let mut recent_items = vec![id];
                    let deps = self.get_reference_cache(id, None, false, false);
                    recent_items.extend(deps);
                    self.append_recent_items(recent_items);
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
                self.save_event(event);
            }
            LedgerEvent::DeleteSet { set } => {
                self.modify_delete_set(set)?;
            }
        };

        Ok(())
    }

    /// Modify the ledger with multiple actions in one go.
    ///
    /// Uses stagingledger for efficiency.
    pub fn modify_action(
        &self,
        id: T::Key,
        action: T::Modifier,
    ) -> Result<CardChange<T>, EventError<T>> {
        let action = ItemAction {
            id,
            action: LedgerAction::Modify(action),
        };
        Ok(self.modify_actions(vec![action])?.remove(0))
    }

    /// Modify the ledger with multiple actions in one go.
    ///
    /// Uses stagingledger for efficiency.
    pub fn modify_actions(
        &self,
        actions: Vec<ItemAction<T>>,
    ) -> Result<Vec<CardChange<T>>, EventError<T>> {
        let mut staging = self.new_staging();

        for action in actions {
            staging.push_event(action)?;
        }

        let changes = staging.commit_events(true, true)?;

        // Track recently modified items and their direct dependencies
        let mut recent_items = Vec::new();
        for change in &changes {
            let id = change.key();
            recent_items.push(id);

            // Add direct dependencies of modified items
            let deps = self.get_reference_cache(id, None, false, false);
            recent_items.extend(deps);
        }
        self.append_recent_items(recent_items);

        Ok(changes)
    }

    pub fn modify_delete_set(&self, set: ItemExpr<T>) -> Result<(), EventError<T>> {
        let dependencies = self.dependents_recursive_set(set.clone());

        if !dependencies.is_empty() {
            return Err(EventError::DeletingWithDependencies(dependencies));
        }

        // Reversed because we must delete dependencies before the dependents.
        let mut keys = self
            .load_set_topologically_sorted(set.clone().into())
            .into_iter()
            .rev();

        let Some(key) = keys.next() else {
            return Ok(());
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
            self.save_event(LedgerEvent::DeleteSet { set });
        }

        Ok(())
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

        self.save_events(applied_events);

        Ok(())
    }

    pub fn save_actions(&self, actions: Vec<ItemAction<T>>) {
        if actions.is_empty() {
            return;
        }
        let entries = NonEmpty::from_vec(actions).unwrap();
        let entry = EventNode::new_branch(entries);
        let hash = self.entries.save_entry(entry);
        self.set_ledger_hash(hash);
    }

    pub fn save_events(&self, events: Vec<impl Into<LedgerEvent<T>>>) {
        let events: Vec<LedgerEvent<T>> = events.into_iter().map(|e| e.into()).collect();
        if events.is_empty() {
            return;
        }
        let mut actions: Vec<ItemAction<T>> = vec![];

        for event in events {
            if !event.is_action() && !actions.is_empty() {
                let taken = std::mem::take(&mut actions);
                self.save_actions(taken);
            } else if let LedgerEvent::ItemAction { id, action } = event {
                actions.push(ItemAction { id, action });
            } else {
                let entry = EventNode::new_leaf(event);
                let hashed = self.entries.save_entry(entry);
                self.set_ledger_hash(hashed);
            }
        }

        if !actions.is_empty() {
            self.save_actions(actions);
        }
    }

    pub fn load_ids(&self) -> IndexSet<T::Key> {
        if self.full_cache() {
            return self.cache.read().unwrap().keys().cloned().collect();
        }

        let mut ids = self.local.load_ids();
        ids.extend(self.remote.load_ids());

        ids
    }

    pub fn all_dependents_with_ty(&self, key: T::Key) -> IndexSet<(T::RefType, T::Key)> {
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

    pub fn get_prop_cache(&self, key: PropertyCache<T>) -> IndexSet<T::Key> {
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
    ) -> IndexSet<T::Key> {
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
    ) -> IndexSet<(T::RefType, T::Key)> {
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
    pub fn dependents_recursive_set(&self, set: ItemExpr<T>) -> IndexSet<T::Key> {
        let items = self.load_expr(set);
        let mut dependencies: IndexSet<T::Key> = IndexSet::new();

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

    pub fn dependencies_recursive(&self, key: T::Key) -> IndexSet<T::Key> {
        let mut items = self.local.recursive_dependencies(key);
        items.extend(self.remote.recursive_dependencies(key));

        items
    }

    pub fn dependents_direct(&self, key: T::Key) -> IndexSet<T::Key> {
        let mut items = self.local.direct_dependents(key);
        items.extend(self.remote.direct_dependents(key));
        items
    }

    pub fn dependents_recursive(&self, key: T::Key) -> IndexSet<T::Key> {
        let mut items = self.local.recursive_dependents(key);
        items.extend(self.remote.recursive_dependents(key));
        items
    }

    pub fn local_load_expr(&self, expr: ItemExpr<T>) -> IndexSet<T::Key> {
        let local = self.local.load_expr(expr.clone());
        let remote = self.remote.load_expr(expr.clone());
        local.difference(&remote).into_iter().cloned().collect()
    }

    pub fn load_expr(&self, expr: ItemExpr<T>) -> IndexSet<T::Key> {
        let mut items = self.local.load_expr(expr.clone());
        items.extend(self.remote.load_expr(expr));
        items
    }

    fn full_cache(&self) -> bool {
        self.full_cache.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn load_all(&self) -> IndexSet<SavedItem<T>> {
        if self.full_cache() {
            return self.cache.read().unwrap().values().cloned().collect();
        }

        // We want to give precedense to remote items. When there are duplicates,
        // extend will keep the items already in place. So it's important we load remote first and extend with local.
        let mut items = self.remote.load_all();
        items.extend(self.local.load_all());

        let mut cache_guard = self.cache.write().unwrap();

        for item in items.clone() {
            cache_guard.insert(item.item_id(), SavedItem::new(item));
        }

        self.full_cache
            .store(true, std::sync::atomic::Ordering::SeqCst);

        drop(cache_guard);

        self.load_all()
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
        if let Some(saved_item) = self.cache.read().unwrap().get(&key).cloned() {
            return Some(saved_item.item().clone());
        }

        match self.remote.load(key).or_else(|| self.local.load(key)) {
            Some(item) => {
                let saved_item = SavedItem::new(item);
                let arc_item = saved_item.item().clone();
                self.cache.write().unwrap().insert(key, saved_item);
                Some(arc_item)
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

    /// Track recently modified/referenced items for quick access.
    /// Keeps the last 100 items that were either modified or referenced as dependencies.
    fn append_recent_items(&self, items: impl IntoIterator<Item = T::Key>) {
        let items: Vec<T::Key> = items.into_iter().collect();
        if items.is_empty() {
            return;
        }

        // Load existing recent items
        let mut recent = self.load_recent_items();

        // Add new items to the front (most recent first)
        for item in items.into_iter().rev() {
            // Remove if already exists to avoid duplicates
            recent.retain(|&x| x != item);
            recent.insert(0, item);
        }

        // Keep only last 100
        recent.truncate(100);

        // Write back
        if let Ok(content) = serde_json::to_string(&recent) {
            let _ = fs::write(&*self.recent_items_path, content);
        }
    }

    /// Load the list of recently modified/referenced items (most recent first).
    pub fn load_recent_items(&self) -> Vec<T::Key> {
        fs::read_to_string(&*self.recent_items_path)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default()
    }

    /// Diagnostic: Returns items grouped by their cluster (weakly connected component).
    ///
    /// Each inner Vec represents one cluster of interconnected items.
    pub fn get_clusters(&self) -> Vec<Vec<T::Key>> {
        let all_ids = self.load_ids();
        if all_ids.is_empty() {
            return vec![];
        }

        let mut visited: IndexSet<T::Key> = IndexSet::new();
        let mut clusters: Vec<Vec<T::Key>> = vec![];

        for id in all_ids.iter() {
            if visited.contains(id) {
                continue;
            }

            // Found a new cluster
            let mut cluster = vec![];
            let mut queue = VecDeque::new();
            queue.push_back(*id);
            visited.insert(*id);

            while let Some(current) = queue.pop_front() {
                cluster.push(current);

                // Follow dependencies (outgoing edges)
                for dep in self.get_reference_cache(current, None, false, false) {
                    if visited.insert(dep) {
                        queue.push_back(dep);
                    }
                }

                // Follow dependents (incoming edges)
                for dependent in self.get_reference_cache(current, None, true, false) {
                    if visited.insert(dependent) {
                        queue.push_back(dependent);
                    }
                }
            }

            clusters.push(cluster);
        }

        clusters
    }

    pub fn verify_all(&self) -> Result<(), EventError<T>> {
        let all = self.load_all();
        let qty = all.len();
        println!("Verifying {} items...", qty);

        let mut seen: BTreeSet<T::Key> = BTreeSet::new();
        for (idx, item) in all.into_iter().enumerate() {
            if seen.contains(&item.item_id()) {
                println!("skipping..");
                continue;
            }

            let id = item.item_id();
            dbg!(id, idx);
            match item.clone_inner().verify_with_deps(self) {
                Ok((_, deps)) => {
                    seen.extend(deps);
                }
                Err(e) => {
                    dbg!(id);
                    dbg!(e);
                    panic!();
                }
            }
        }

        println!("All items verified successfully.");

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
            if idx % 1 == 0 {
                dbg!(idx);
            };

            if entry.is_branch() {
                let event_node = entry.to_event_node();
                for ch in
                    StagingLedger::push_commit_node(self.clone(), event_node, false, false).unwrap()
                {
                    match ch {
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
                continue;
            }

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

        simpletime::timed!(self.verify_all().unwrap());

        if let Some(hash) = self.entries.current_hash() {
            info!("{} ledger hash after apply: {}", Self::item_name(), hash);
            self.set_ledger_hash(hash);
        }
    }

    fn apply_caches(&self, items: HashMap<T::Key, Arc<T>>) {
        info!("applying caches");
        let mut the_caches: HashMap<CacheKey<T>, IndexSet<T::Key>> = Default::default();

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
            CacheKey::Right(ItemRefCache {
                reftype,
                referent: to,
            }) => {
                self.insert_reference(ItemReference::new(id, to, reftype));
            }
        }
    }

    fn remove_cache(&self, cache: CacheKey<T>, id: T::Key) {
        match cache {
            CacheKey::Left(PropertyCache { property, value }) => {
                self.remove_property(id, property, value);
            }
            CacheKey::Right(ItemRefCache {
                reftype,
                referent: to,
            }) => {
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
            my_hard_link(&original, &link);
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
        my_hard_link(&original, &link);
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
        my_hard_link(&original, &link);
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

        let mut dependents: IndexSet<T::Key> = IndexSet::default();

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

                (
                    IndexSet::default(),
                    caches,
                    CardChange::Created(item),
                    false,
                )
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
                    let saved_item = SavedItem::new(Arc::unwrap_or_clone(item));
                    self.cache
                        .write()
                        .unwrap()
                        .insert(saved_item.item_id(), saved_item.clone());

                    self.save(saved_item.clone_inner());
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

impl<T: LedgerItem> ReadLedger for Ledger<T> {
    type Item = T;

    fn load(&self, key: <Self::Item as LedgerItem>::Key) -> Option<Self::Item> {
        self.load(key).map(|x| (*x).clone())
    }

    fn load_ids(&self) -> IndexSet<<Self::Item as LedgerItem>::Key> {
        self.load_ids()
    }

    fn get_property_cache(
        &self,
        cache: PropertyCache<Self::Item>,
    ) -> IndexSet<<Self::Item as LedgerItem>::Key> {
        self.get_prop_cache(cache)
    }

    fn get_reference_cache(
        &self,
        key: <Self::Item as LedgerItem>::Key,
        ty: Option<<Self::Item as LedgerItem>::RefType>,
        reversed: bool,
        recursive: bool,
    ) -> IndexSet<<Self::Item as LedgerItem>::Key> {
        self.get_reference_cache(key, ty, reversed, recursive)
    }

    fn get_reference_cache_with_ty(
        &self,
        key: <Self::Item as LedgerItem>::Key,
        ty: Option<<Self::Item as LedgerItem>::RefType>,
        reversed: bool,
        recursive: bool,
    ) -> IndexSet<(
        <Self::Item as LedgerItem>::RefType,
        <Self::Item as LedgerItem>::Key,
    )> {
        self.get_reference_cache_with_ty(key, ty, reversed, recursive)
    }
}

fn my_hard_link(src: &Path, dst: &Path) {
    match std::fs::hard_link(src, dst) {
        Ok(()) => {}
        Err(e) => match e.kind() {
            io::ErrorKind::AlreadyExists => {}
            e => {
                dbg!(src, dst);
                panic!("Error creating reference link: {:?}", e);
            }
        },
    }
}
