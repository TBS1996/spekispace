use chrono::{DateTime, Utc};
use either::Either;
use git2::build::CheckoutBuilder;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::fs::{self, hard_link};
use std::io::Write;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::vec::Vec;
use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    hash::{DefaultHasher, Hash, Hasher},
    sync::{Arc, RwLock},
};
use tracing::info;
use uuid::Uuid;

mod blockchain;
pub mod ledger_cache;
mod ledger_item;
mod read_ledger;
pub type CacheKey<T> = Either<PropertyCache<T>, ItemRefCache<T>>;
use blockchain::BlockChain;

pub use ledger_item::LedgerItem;

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

#[derive(Clone)]
pub struct RefGetter<T: LedgerItem> {
    pub reversed: bool, // whether it fetches links from the item to other items or the way this item being referenced
    pub key: T::Key,    // item in question
    pub ty: Option<T::RefType>, // the way of linking. None means all.
    pub recursive: bool, // recursively get all cards that link
}

/// Represents a way to fetch an item based on either its properites
#[derive(Clone)]
pub enum TheCacheGetter<T: LedgerItem> {
    ItemRef(RefGetter<T>),
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

#[derive(Debug, Clone)]
pub enum EventError<T: LedgerItem> {
    Cycle(Vec<(T::Key, T::RefType)>),
    Invariant(T::Error),
    ItemNotFound,
    DeletingWithDependencies,
    Remote,
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
    pub event: LedgerEvent<T>,
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
#[serde(untagged)]
#[serde(bound(
    serialize = "T: LedgerItem + Serialize,
                   T::Key: Serialize,
                   LedgerAction<T>: Serialize",
    deserialize = "T: LedgerItem + DeserializeOwned,
                   T::Key: DeserializeOwned,
                   LedgerAction<T>: DeserializeOwned",
))]
pub enum LedgerEvent<T: LedgerItem> {
    ItemAction { id: T::Key, action: LedgerAction<T> },
    SetUpstream { commit: String },
}

impl<T: LedgerItem> LedgerEvent<T> {
    pub fn new(id: T::Key, action: LedgerAction<T>) -> Self {
        Self::ItemAction { id, action }
    }

    pub fn new_modify(id: T::Key, action: T::Modifier) -> Self {
        Self::ItemAction {
            id,
            action: LedgerAction::Modify(action),
        }
    }

    pub fn new_delete(id: T::Key) -> Self {
        Self::ItemAction {
            id,
            action: LedgerAction::Delete,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, Hash, PartialEq)]
#[serde(bound(deserialize = "T: LedgerItem + DeserializeOwned"))]
pub enum LedgerAction<T: LedgerItem> {
    Create(T),
    Modify(T::Modifier),
    Delete,
}

pub enum LedgerType<T: LedgerItem> {
    OverRide(OverrideLedger<T>),
    Normal(Ledger<T>),
}

impl<T: LedgerItem> LedgerType<T> {
    pub fn load(&self, key: T::Key) -> Option<T> {
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

    pub fn load(&self, key: T::Key) -> Option<T> {
        if self.new_id == key {
            Some(self.new.clone())
        } else {
            self.inner.load(key)
        }
    }

    pub fn dependencies(&self, key: T::Key) -> HashSet<T::Key> {
        self.load(key).unwrap().dependencies()
    }

    pub fn dependents(&self, key: T::Key) -> HashSet<T::Key> {
        let mut dependents = self.inner.dependents_recursive(key);

        if self.new.dependencies().contains(&key) {
            dependents.insert(self.new_id);
        }

        dependents
    }
}

struct Remote<T: LedgerItem> {
    _url: String,
    repo: Repository,
    path: Arc<PathBuf>,
    _phantom: PhantomData<T>,
}

use git2::{Delta, DiffFindOptions, DiffOptions, ObjectType, Oid, Repository, ResetType};

use crate::read_ledger::ReadLedger;

#[derive(Debug)]
pub struct ChangeSet<T> {
    pub added: Vec<T>,
    pub modified: Vec<T>,
    pub removed: Vec<T>,
}

impl<T> Default for ChangeSet<T> {
    fn default() -> Self {
        Self {
            added: Default::default(),
            modified: Default::default(),
            removed: Default::default(),
        }
    }
}

impl<T: LedgerItem> ReadLedger for Remote<T> {
    type Item = T;

    fn root_path(&self) -> PathBuf {
        self.path.to_path_buf()
    }
}

impl<T: LedgerItem> Remote<T> {
    pub fn new(root: &Path, url: String) -> Self {
        let path = root.join("remote");
        fs::create_dir_all(&path).unwrap();

        let repo = match Repository::open(&path) {
            Ok(r) => {
                if r.find_remote("origin").is_ok() {
                    r.remote_set_url("origin", &url).unwrap();
                } else {
                    r.remote("origin", &url).unwrap();
                }
                r
            }
            Err(_) => {
                let r = Repository::init(&path).unwrap();
                r.remote("origin", &url).unwrap();
                r
            }
        };

        Self {
            _url: url,
            repo,
            path: Arc::new(path),
            _phantom: PhantomData,
        }
    }

    pub fn current_commit(&self) -> Option<String> {
        match self.repo.head() {
            Ok(head) => Some(head.peel_to_commit().unwrap().id().to_string()),
            Err(_) => None,
        }
    }

    pub fn latest_upstream_commit(&self) -> String {
        // Fetch the remote main branch (won’t update worktree)
        let mut remote = self.repo.find_remote("origin").unwrap();
        remote
            .fetch(&["refs/heads/main:refs/remotes/origin/main"], None, None)
            .unwrap();

        // Read the commit ID from the remote-tracking branch
        let reference = self
            .repo
            .find_reference("refs/remotes/origin/main")
            .unwrap();
        let oid = reference.target().unwrap();

        oid.to_string()
    }

    pub fn current_commit_date(&self) -> Option<DateTime<Utc>> {
        let commit = self.repo.head().ok()?.peel_to_commit().ok()?;

        let timestamp = commit.time().seconds();
        let naive = DateTime::from_timestamp(timestamp, 0)?;
        Some(naive)
    }

    pub fn changed_paths(&self, old_commit: Option<&str>, new_commit: &str) -> ChangeSet<PathBuf> {
        // resolve new
        let new_oid = Oid::from_str(new_commit).unwrap();
        let new_commit = self
            .repo
            .find_object(new_oid, Some(ObjectType::Commit))
            .unwrap()
            .into_commit()
            .unwrap();
        let new_tree = new_commit.tree().unwrap();

        // resolve old (or empty)
        let old_tree = old_commit.map(|s| {
            let old_oid = Oid::from_str(s).unwrap();
            let old_commit = self
                .repo
                .find_object(old_oid, Some(ObjectType::Commit))
                .unwrap()
                .into_commit()
                .unwrap();
            old_commit.tree().unwrap()
        });

        let mut opts = DiffOptions::new();
        let mut diff = self
            .repo
            .diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), Some(&mut opts))
            .unwrap();

        // detect renames/copies
        let mut find = DiffFindOptions::new();
        find.renames(true).renames_from_rewrites(true);
        diff.find_similar(Some(&mut find)).unwrap();

        let mut out = ChangeSet {
            added: vec![],
            modified: vec![],
            removed: vec![],
        };

        diff.foreach(
            &mut |d, _| {
                match d.status() {
                    Delta::Added | Delta::Copied => {
                        let p = d.new_file().path().unwrap().to_path_buf();
                        out.added.push(p);
                    }
                    Delta::Modified | Delta::Typechange => {
                        let p = d.new_file().path().unwrap().to_path_buf();
                        out.modified.push(p);
                    }
                    Delta::Deleted => {
                        let p = d.old_file().path().unwrap().to_path_buf();
                        out.removed.push(p);
                    }
                    Delta::Renamed => {
                        let oldp = d.old_file().path().unwrap().to_path_buf();
                        let newp = d.new_file().path().unwrap().to_path_buf();
                        out.removed.push(oldp);
                        out.added.push(newp);
                    }
                    // Untracked/Unmodified/Ignored/Conflicted → skip
                    _ => {}
                }
                true
            },
            None,
            None,
            None,
        )
        .unwrap();

        out
    }

    fn modified_items(&self, old_commit: Option<&str>, new_commit: &str) -> ChangeSet<T::Key> {
        let mut changed_items: ChangeSet<T::Key> = Default::default();

        let changed_paths = self.changed_paths(old_commit, new_commit);

        for path in changed_paths.added {
            if path.starts_with("items") {
                let filename = path.file_name().unwrap().to_str().unwrap();
                dbg!(&filename);
                match filename.parse() {
                    Ok(key) => {
                        changed_items.added.push(key);
                    }
                    Err(_) => {
                        dbg!(&filename);
                        panic!();
                    }
                }
            }
        }

        for path in changed_paths.modified {
            if path.starts_with("items") {
                let filename = path.file_name().unwrap().to_str().unwrap();
                dbg!(&filename);
                match filename.parse() {
                    Ok(key) => {
                        changed_items.modified.push(key);
                    }
                    Err(_) => {
                        dbg!(&filename);
                        panic!();
                    }
                }
            }
        }
        for path in changed_paths.removed {
            if path.starts_with("items") {
                let filename = path.file_name().unwrap().to_str().unwrap();
                dbg!(&filename);
                match filename.parse() {
                    Ok(key) => {
                        changed_items.removed.push(key);
                    }
                    Err(_) => {
                        dbg!(&filename);
                        panic!();
                    }
                }
            }
        }

        changed_items
    }

    pub fn hard_reset_current(&self) -> Result<(), git2::Error> {
        let head = self.repo.head()?;
        let obj = head.peel(git2::ObjectType::Commit)?;

        let mut cb = CheckoutBuilder::new();
        cb.force()
            .remove_untracked(true)
            .remove_ignored(true)
            .recreate_missing(true);

        self.repo.checkout_tree(&obj, Some(&mut cb))?;
        self.repo.reset(&obj, ResetType::Hard, Some(&mut cb))?;

        let _ = self.repo.cleanup_state();
        Ok(())
    }

    pub fn reset_to_empty(&self) {
        let refs = self.repo.references().unwrap();
        for r in refs {
            let r = r.unwrap();
            if let Some(name) = r.name() {
                if name.starts_with("refs/heads/") || name.starts_with("refs/tags/") {
                    self.repo.find_reference(name).unwrap().delete().unwrap();
                }
            }
        }

        self.repo.set_head("refs/heads/main").unwrap();

        let mut cb = git2::build::CheckoutBuilder::new();
        cb.force().remove_untracked(true).remove_ignored(true);
        self.repo.checkout_head(Some(&mut cb)).unwrap();
    }

    pub fn set_commit_clean(&self, commit: &str) -> Result<ChangeSet<T::Key>, git2::Error> {
        let curr_commit = self.current_commit();

        {
            let mut remote = self.repo.find_remote("origin").unwrap();
            remote.fetch::<&str>(&[], None, None).unwrap();
        }

        let oid = Oid::from_str(commit).unwrap();
        self.repo.set_head_detached(oid).unwrap();

        self.hard_reset_current().unwrap();
        assert_eq!(self.current_commit().unwrap().as_str(), commit);

        let diffs = self.modified_items(curr_commit.as_deref(), commit);
        dbg!(&diffs);
        Ok(diffs)
    }
}

#[derive(Debug, Clone)]
struct ModifyResult<T: LedgerItem> {
    key: T::Key,
    item: Option<T>,
    added_caches: HashSet<(CacheKey<T>, T::Key)>,
    removed_caches: HashSet<(CacheKey<T>, T::Key)>,
    is_no_op: bool,
}

#[derive(Clone)]
struct Local<T: LedgerItem> {
    root: Arc<PathBuf>,
    _phantom: PhantomData<T>,
}

impl<T: LedgerItem> ReadLedger for Local<T> {
    type Item = T;

    fn root_path(&self) -> PathBuf {
        self.root.to_path_buf()
    }
}

#[derive(Clone)]
pub struct Ledger<T: LedgerItem> {
    entries: BlockChain<T>,
    properties: Arc<PathBuf>,
    dependencies: Arc<PathBuf>,
    dependents: Arc<PathBuf>,
    items: Arc<PathBuf>,
    ledger_hash: Arc<PathBuf>,
    remote: Option<Arc<Remote<T>>>,
    local: Local<T>,
    root: Arc<PathBuf>,
}

impl<T: LedgerItem> Ledger<T> {
    pub fn new(root: PathBuf) -> Self {
        let root = root.join(Self::item_name());
        let entries = BlockChain::new(root.join("entries"));
        let root = root.join("state");

        let properties = root.join("properties");
        let dependencies = root.join("dependencies");
        let dependents = root.join("dependents");
        let items = root.join("items");
        let ledger_hash = root.join("applied");

        std::fs::create_dir_all(&properties).unwrap();
        std::fs::create_dir_all(&dependencies).unwrap();
        std::fs::create_dir_all(&dependents).unwrap();
        std::fs::create_dir_all(&items).unwrap();

        let selv = Self {
            properties: Arc::new(properties),
            dependencies: Arc::new(dependencies),
            dependents: Arc::new(dependents),
            items: Arc::new(items.clone()),
            ledger_hash: Arc::new(ledger_hash),
            entries,
            remote: None,
            root: Arc::new(root.clone()),
            local: Local {
                root: Arc::new(root.clone()),
                _phantom: PhantomData,
            },
        };

        if selv.entries.current_hash() != selv.currently_applied_ledger_hash() {
            selv.apply();
        }

        selv
    }

    pub fn current_commit_date(&self) -> Option<DateTime<Utc>> {
        self.remote.as_ref()?.current_commit_date()
    }

    pub fn current_commit(&self) -> Option<String> {
        self.remote.as_ref()?.current_commit()
    }

    pub fn latest_upstream_commit(&self) -> Option<String> {
        Some(self.remote.as_ref()?.latest_upstream_commit())
    }

    pub fn modify(&self, event: LedgerEvent<T>) -> Result<(), EventError<T>> {
        self.modify_it(event, true, true, true)?;
        Ok(())
    }

    pub fn load_ids(&self) -> HashSet<T::Key> {
        let mut ids = self.local.load_ids();
        if let Some(remote) = self.remote.as_ref() {
            ids.extend(remote.load_ids());
        }

        ids
    }

    pub fn all_dependents_with_ty(&self, key: T::Key) -> HashSet<(T::RefType, T::Key)> {
        let mut items = self.local.all_dependents_with_ty(key);
        if let Some(remote) = self.remote.as_ref() {
            items.extend(remote.all_dependents_with_ty(key));
        }

        items
    }

    pub fn get_prop_cache(&self, key: PropertyCache<T>) -> HashSet<T::Key> {
        let mut items = self.local.get_prop_cache(key.clone());
        if let Some(remote) = self.remote.as_ref() {
            items.extend(remote.get_prop_cache(key));
        }

        items
    }

    pub fn dependencies_recursive(&self, key: T::Key) -> HashSet<T::Key> {
        let mut items = self.local.recursive_dependencies(key);
        if let Some(remote) = self.remote.as_ref() {
            items.extend(remote.recursive_dependencies(key));
        }

        items
    }

    pub fn dependents_recursive(&self, key: T::Key) -> HashSet<T::Key> {
        let mut items = self.local.recursive_dependents(key);
        if let Some(remote) = self.remote.as_ref() {
            items.extend(remote.recursive_dependents(key));
        }
        items
    }

    pub fn load_getter(&self, getter: TheCacheGetter<T>) -> HashSet<T::Key> {
        let mut items = self.local.load_getter(getter.clone());
        if let Some(remote) = self.remote.as_ref() {
            items.extend(remote.load_getter(getter));
        }
        items
    }

    pub fn with_remote(mut self, url: String) -> Self {
        let remote = Remote::new(&self.root, url);
        let _ = remote.hard_reset_current();
        self.remote = Some(Arc::new(remote));
        self
    }

    pub fn load_all(&self) -> HashSet<T> {
        let mut items = self.local.load_all();
        if let Some(remote) = self.remote.as_ref() {
            items.extend(remote.load_all());
        }

        items
    }

    pub fn load_with_remote_info(&self, key: T::Key) -> Option<(T, bool)> {
        if let Some(remote) = self.remote.as_ref() {
            let item = remote.load(key);
            if item.is_some() {
                return item.map(|item| (item, true));
            }
        }

        self.local.load(key).map(|item| (item, false))
    }

    pub fn load(&self, key: T::Key) -> Option<T> {
        if let Some(remote) = self.remote.as_ref() {
            let item = remote.load(key);
            if item.is_some() {
                return item;
            }
        }

        self.local.load(key)
    }

    pub fn currently_applied_ledger_hash(&self) -> Option<LedgerHash> {
        fs::read_to_string(&*self.ledger_hash).ok()
    }

    fn item_path(&self, key: T::Key) -> PathBuf {
        if let Some(remote) = self.remote.as_ref() {
            let p = remote.item_path(key);
            if p.is_file() {
                return p;
            }
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

    fn apply(&self) {
        fs::remove_dir_all(&*self.items).unwrap();
        fs::remove_dir_all(&*self.properties).unwrap();
        fs::remove_dir_all(&*self.dependencies).unwrap();
        fs::remove_dir_all(&*self.dependents).unwrap();

        fs::create_dir(&*self.items).unwrap();
        fs::create_dir(&*self.properties).unwrap();
        fs::create_dir(&*self.dependencies).unwrap();
        fs::create_dir(&*self.dependents).unwrap();

        let mut items: HashMap<T::Key, T> = HashMap::default();

        for (idx, entry) in self.entries.chain().into_iter().enumerate() {
            if idx % 50 == 0 {
                dbg!(idx);
            };

            match self.modify_it(entry.event, false, false, false).unwrap() {
                Some(Either::Left(item)) => {
                    let key = item.item_id();
                    items.insert(key, item);
                }
                Some(Either::Right(id)) => {
                    items.remove(&id);
                }
                None => {}
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
        let depencies_dir = self.local.root_dependencies_dir(id);
        fs::remove_dir_all(&depencies_dir).unwrap();
        let depencies_dir = self.local.root_dependencies_dir(id); //recreate it

        for ItemReference { from: _, to, ty } in item.ref_cache() {
            let dir = depencies_dir.join(ty.to_string());
            fs::create_dir_all(&dir).unwrap();
            let original = self.item_path(to);
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

    fn set_ledger_hash(&self, hash: LedgerHash) {
        let mut f = fs::File::create(&*self.ledger_hash).unwrap();
        f.write_all(hash.as_bytes()).unwrap();
    }

    fn remove_dependent(&self, key: T::Key, ty: T::RefType, dependent: T::Key) {
        let path = self
            .local
            .dependents_dir(key, ty.clone())
            .join(dependent.to_string());
        match fs::remove_file(&path) {
            Ok(_) => {}
            Err(e) => {
                dbg!(&path);
                dbg!(key, ty, dependent);
                dbg!(e);
                debug_assert!(false);
            }
        }
    }

    fn remove_property(&self, key: T::Key, property: T::PropertyType, value: String) {
        let cache = CacheKey::Left(PropertyCache { property, value });
        self.remove_cache(cache, key);
    }

    fn insert_cache(&self, cache: CacheKey<T>, id: T::Key) {
        let path = self.local.cache_dir(cache);
        fs::create_dir_all(&path).unwrap();
        let original = self.item_path(id);
        let link = path.join(id.to_string());
        hard_link(original, link).unwrap();
    }

    fn remove_cache(&self, cache: CacheKey<T>, id: T::Key) {
        let path = self.local.cache_dir(cache.clone()).join(id.to_string());
        let _res = fs::remove_file(&path);

        if let Err(e) = _res {
            dbg!(&path, e, cache, id);
            panic!();
        }
    }

    fn is_remote(&self, key: T::Key) -> bool {
        match self.remote.as_ref() {
            Some(remote) => remote.load(key).is_some(),
            None => false,
        }
    }

    fn set_remote_commit(&self, new_commit: &str) -> Result<(), EventError<T>> {
        let remote = self.remote.as_ref().unwrap().clone();

        let current_commit = remote.current_commit();
        let ChangeSet {
            added: _,
            modified,
            removed,
        } = remote.set_commit_clean(new_commit).unwrap();

        for item in removed {
            if !self.local.direct_dependents(item).is_empty() {
                match current_commit {
                    Some(commit) => {
                        let _ = remote.set_commit_clean(&commit).unwrap();
                    }
                    None => {
                        remote.reset_to_empty();
                    }
                }

                return Err(EventError::ItemNotFound);
            }
        }

        let mut dependents: HashSet<T::Key> = HashSet::default();

        for item in &modified {
            dependents.extend(self.local.direct_dependents(*item));
        }

        dbg!(&modified, &dependents);

        for dependent in dependents {
            let item = self.local.load(dependent).unwrap();

            let _ = match item.verify(self) {
                Ok(item) => item,
                Err(e) => {
                    match current_commit {
                        Some(commit) => {
                            let _ = remote.set_commit_clean(&commit).unwrap();
                        }
                        None => {
                            remote.reset_to_empty();
                        }
                    }
                    return Err(e);
                }
            };
        }

        Ok(())
    }

    fn _modify(
        &self,
        key: T::Key,
        action: LedgerAction<T>,
        verify: bool,
        cache: bool,
    ) -> Result<ModifyResult<T>, EventError<T>> {
        if self.is_remote(key) {
            return Err(EventError::Remote);
        }

        let (old_caches, new_caches, item, is_no_op) = match action.clone() {
            LedgerAction::Modify(action) => {
                let (old_caches, old_item) = match self.load(key) {
                    Some(item) if cache => (item.caches(self), item),
                    Some(item) => (Default::default(), item),
                    None => (Default::default(), T::new_default(key)),
                };
                let old_cloned = old_item.clone();
                let modified_item = old_item.run_event(action, self, verify)?;
                let no_op = old_cloned == modified_item;
                let new_caches = modified_item.caches(self);
                (old_caches, new_caches, Some(modified_item), no_op)
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
                (HashSet::default(), caches, Some(item), false)
            }
            LedgerAction::Delete => {
                let old_item = self.load(key).unwrap();
                let old_caches = old_item.caches(self);
                (old_caches, Default::default(), None, false)
            }
        };

        let added_caches = &new_caches - &old_caches;
        let removed_caches = &old_caches - &new_caches;

        Ok(ModifyResult {
            key,
            item,
            added_caches,
            removed_caches,
            is_no_op,
        })
    }

    fn run_result(&self, res: ModifyResult<T>, cache: bool) -> Result<(), EventError<T>> {
        let ModifyResult {
            key,
            item,
            added_caches,
            removed_caches,
            is_no_op,
        } = res;

        if is_no_op {
            if !added_caches.is_empty() || !removed_caches.is_empty() {
                dbg!(key, &item, &added_caches, &removed_caches);
            }
        }

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
                    self.remove_dependent(id, reftype, key);
                }
                CacheKey::Left(PropertyCache { property, value }) => {
                    self.remove_property(key, property, value);
                }
            }
        }

        Ok(())
    }

    fn modify_it(
        &self,
        event: LedgerEvent<T>,
        save: bool,
        verify: bool,
        cache: bool,
    ) -> Result<Option<Either<T, T::Key>>, EventError<T>> {
        let res = match event.clone() {
            LedgerEvent::ItemAction { id, action } => self._modify(id, action, verify, cache)?,
            LedgerEvent::SetUpstream { commit } => {
                self.set_remote_commit(&commit)?;
                let hash = self.entries.save(event);
                self.set_ledger_hash(hash);
                return Ok(None);
            }
        };
        tracing::debug!("res: {:?}", &res);
        self.run_result(res.clone(), cache).unwrap();
        if save && !res.is_no_op {
            let hash = self.entries.save(event);
            self.set_ledger_hash(hash);
        }
        Ok(Some(match res.item {
            Some(item) => Either::Left(item),
            None => Either::Right(res.key),
        }))
    }

    fn remove(&self, key: T::Key) {
        let path = self.item_path(key);
        std::fs::remove_file(path).unwrap();
    }

    fn save(&self, item: T) {
        let key = item.item_id();
        let item_path = self.item_path(key);
        let serialized = serde_json::to_string_pretty(&item).unwrap();
        let mut f = std::fs::File::create(&item_path).unwrap();
        use std::io::Write;

        f.write_all(serialized.as_bytes()).unwrap();
        self.set_dependencies(&item);
    }
}

impl<T: LedgerItem> LedgerEntry<T> {
    fn new(previous: Option<&Self>, event: LedgerEvent<T>) -> Self {
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
