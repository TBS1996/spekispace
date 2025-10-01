use chrono::{DateTime, Utc};
use either::Either;
use git2::build::CheckoutBuilder;
use nonempty::NonEmpty;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::fs::{self, hard_link};
use std::io::Write;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
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
use blockchain::BlockChain;
pub mod entry_thing;

pub use ledger_item::LedgerItem;

pub type CacheKey<T> = Either<PropertyCache<T>, ItemRefCache<T>>;
pub type Blob = Vec<u8>;

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
    ItemNotFound(T::Key),
    DeletingWithDependencies,
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

#[derive(Clone, Serialize, Deserialize, Debug, Hash)]
#[serde(bound(deserialize = "T: LedgerItem + DeserializeOwned,
                   T::Key: DeserializeOwned"))]
pub struct LedgerEntry<T: LedgerItem> {
    pub previous: Option<Hashed>,
    pub index: usize,
    pub event: LedgerEvent<T>,
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
    ItemAction {
        id: T::Key,
        action: LedgerAction<T>,
    },
    SetUpstream {
        commit: String,
        upstream_url: String,
    },
}

struct ItemAction<T: LedgerItem> {
    id: T::Key,
    action: LedgerAction<T>,
}

#[derive(Clone, Debug)]
struct SetUpstream {
    commit: String,
    upstream_url: String,
}

impl<T: LedgerItem> From<ItemAction<T>> for LedgerEvent<T> {
    fn from(value: ItemAction<T>) -> Self {
        let ItemAction { id, action } = value;

        Self::ItemAction { id, action }
    }
}

impl<T: LedgerItem> From<SetUpstream> for LedgerEvent<T> {
    fn from(value: SetUpstream) -> Self {
        let SetUpstream {
            commit,
            upstream_url,
        } = value;

        Self::SetUpstream {
            commit,
            upstream_url,
        }
    }
}

impl<T: LedgerItem> LedgerEvent<T> {
    fn into_parts(self) -> Either<SetUpstream, ItemAction<T>> {
        match self {
            LedgerEvent::ItemAction { id, action } => Either::Right(ItemAction { id, action }),
            LedgerEvent::SetUpstream {
                commit,
                upstream_url,
            } => Either::Left(SetUpstream {
                commit,
                upstream_url,
            }),
        }
    }

    fn data_hash(&self) -> Hashed {
        get_hash(self)
    }

    pub fn new(id: T::Key, action: LedgerAction<T>) -> Self {
        Self::ItemAction { id, action }
    }

    pub fn id(&self) -> Option<T::Key> {
        match self {
            LedgerEvent::ItemAction { id, .. } => Some(*id),
            LedgerEvent::SetUpstream { .. } => None,
        }
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

struct Remote<T: LedgerItem> {
    repo: Repository,
    path: Arc<PathBuf>,
    _phantom: PhantomData<T>,
}

use git2::{Delta, DiffFindOptions, DiffOptions, ObjectType, Oid, Repository, ResetType};

use crate::entry_thing::EntryThing;
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
    pub fn new(root: &Path) -> Self {
        let path = root.join("remote");
        fs::create_dir_all(&path).unwrap();

        let repo = match Repository::open(&path) {
            Ok(r) => r,
            Err(_) => Repository::init(&path).unwrap(),
        };

        Self {
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

    fn remote_min_version(
        oid: &str,
        github_user: &str,
        github_repo: &str,
    ) -> Option<semver::Version> {
        let url = format!(
            "https://raw.githubusercontent.com/{github_user}/{github_repo}/{oid}/min_version",
        );

        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(1))
            .build();

        let version: semver::Version = agent
            .get(&url)
            .call()
            .ok()?
            .into_string()
            .unwrap()
            .trim()
            .parse()
            .unwrap();

        Some(version)
    }

    pub fn latest_upstream_commit(&self, url: &str) -> Option<String> {
        let mut r = self.repo.remote_anonymous(url).ok()?;
        r.connect(git2::Direction::Fetch).ok()?;

        let heads = r.list().ok()?;

        let want1 = r
            .default_branch()
            .ok()
            .and_then(|b| b.as_str().map(str::to_owned));

        let want2 = heads.iter().find_map(|h| {
            if h.name() == "HEAD" {
                h.symref_target().map(|s| s.to_string())
            } else {
                None
            }
        });

        let want = want1
            .or(want2)
            .or_else(|| Some("refs/heads/main".to_string()));

        let oid = if let Some(ref want) = want {
            heads.iter().find_map(|h| {
                if h.name() == want.as_str() {
                    Some(h.oid())
                } else {
                    None
                }
            })
        } else {
            None
        }
        .or_else(|| {
            heads.iter().find_map(|h| {
                if h.name() == "refs/heads/master" {
                    Some(h.oid())
                } else {
                    None
                }
            })
        })?;

        Some(oid.to_string())
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

    #[allow(unused)]
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

    pub fn reset_to_first_commit(&self) -> Result<(), git2::Error> {
        // Start at HEAD
        let mut c = self.repo.head()?.peel_to_commit()?;

        // Walk back to the root (no parents)
        while c.parent_count() > 0 {
            c = c.parent(0)?;
        }

        // Move HEAD and update index + worktree to that commit
        self.repo.reset(c.as_object(), ResetType::Hard, None)?;
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

    pub fn set_commit_clean(
        &self,
        upstream_url: Option<&str>,
        commit: &str,
    ) -> Result<ChangeSet<T::Key>, git2::Error> {
        let curr_commit = self.current_commit();

        if let Some(upstream_url) = upstream_url {
            self.fetch_heads_anonymous(upstream_url)?;
        }

        let oid = Oid::from_str(commit)?;
        let _obj = self.repo.find_object(oid, None)?;

        self.repo.set_head_detached(oid)?;
        let obj = self.repo.find_object(oid, None)?;
        self.repo.reset(&obj, ResetType::Hard, None)?;

        debug_assert_eq!(self.current_commit().as_deref(), Some(commit));

        let diffs = self.modified_items(curr_commit.as_deref(), commit);
        Ok(diffs)
    }

    fn fetch_heads_anonymous(&self, url: &str) -> Result<(), git2::Error> {
        use git2::AutotagOption;
        use git2::FetchOptions;

        let mut remote = self.repo.remote_anonymous(url)?;

        let mut opts = FetchOptions::new();
        opts.download_tags(AutotagOption::None);

        let refspecs = ["+refs/heads/*:refs/remotes/__tmp/*"];

        remote.fetch(&refspecs, Some(&mut opts), None)
    }
}

#[derive(Debug, Clone)]
struct ActionEvalResult<T: LedgerItem> {
    item: CardChange<T>,
    added_caches: HashSet<(CacheKey<T>, T::Key)>,
    removed_caches: HashSet<(CacheKey<T>, T::Key)>,
    is_no_op: bool,
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
    properties: Arc<PathBuf>,
    dependencies: Arc<PathBuf>,
    dependents: Arc<PathBuf>,
    items: Arc<PathBuf>,
    ledger_hash: Arc<PathBuf>,
    remote: Arc<Remote<T>>,
    local: Local<T>,
    cache: Arc<RwLock<HashMap<T::Key, Arc<T>>>>,
    full_cache: Arc<AtomicBool>,
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

        let remote = Remote::new(&root);
        //let _ = remote.hard_reset_current();

        let selv = Self {
            properties: Arc::new(properties),
            dependencies: Arc::new(dependencies),
            dependents: Arc::new(dependents),
            items: Arc::new(items.clone()),
            ledger_hash: Arc::new(ledger_hash),
            entries,
            remote: Arc::new(remote),
            cache: Default::default(),
            full_cache: Arc::new(AtomicBool::new(false)),
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
        self.remote.current_commit_date()
    }

    pub fn current_commit(&self) -> Option<String> {
        self.remote.current_commit()
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
            match event.clone().into_parts() {
                Either::Left(set_upstream) => {
                    self.apply_and_save_upstream_commit(set_upstream)?;
                    applied_events.push(event);
                }
                Either::Right(ItemAction { id, action }) => {
                    let verify = true;
                    let cache = true;

                    let evaluation = self.evaluate_action(id, action.clone(), verify, cache)?;

                    tracing::debug!("res: {:?}", &evaluation);

                    if !evaluation.is_no_op {
                        self.apply_evaluation(evaluation.clone(), cache).unwrap();
                        applied_events.push(event);
                    }
                }
            }
        }

        if applied_events.is_empty() {
            Ok(())
        } else if applied_events.len() == 1 {
            let entry = applied_events.remove(0);
            let entry = EntryThing::new_single(entry);
            self.entries.save_entry(entry);
            Ok(())
        } else {
            let entries = NonEmpty::from_vec(applied_events).unwrap();
            let entry = EntryThing::new_multiple(entries);
            self.entries.save_entry(entry);
            Ok(())
        }
    }

    pub fn load_ids(&self) -> HashSet<T::Key> {
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
        let mut items = self.local.get_prop_cache(key.clone());
        items.extend(self.remote.get_prop_cache(key));

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

    pub fn dependencies_recursive(&self, key: T::Key) -> HashSet<T::Key> {
        let mut items = self.local.recursive_dependencies(key);
        items.extend(self.remote.recursive_dependencies(key));

        items
    }

    pub fn dependents_recursive(&self, key: T::Key) -> HashSet<T::Key> {
        let mut items = self.local.recursive_dependents(key);
        items.extend(self.remote.recursive_dependents(key));
        items
    }

    pub fn load_getter(&self, getter: TheCacheGetter<T>) -> HashSet<T::Key> {
        let mut items = self.local.load_getter(getter.clone());
        items.extend(self.remote.load_getter(getter));
        items
    }

    pub fn load_all(&self) -> HashSet<T> {
        if self.full_cache.load(std::sync::atomic::Ordering::SeqCst) {
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
    fn apply(&self) {
        fs::remove_dir_all(&*self.items).unwrap();
        fs::remove_dir_all(&*self.properties).unwrap();
        fs::remove_dir_all(&*self.dependencies).unwrap();
        fs::remove_dir_all(&*self.dependents).unwrap();

        fs::create_dir(&*self.items).unwrap();
        fs::create_dir(&*self.properties).unwrap();
        fs::create_dir(&*self.dependencies).unwrap();
        fs::create_dir(&*self.dependents).unwrap();

        let mut items: HashMap<T::Key, Arc<T>> = HashMap::default();

        self.remote.reset_to_first_commit().unwrap();

        for (idx, entry) in self.entries.chain().into_iter().enumerate() {
            if idx % 50 == 0 {
                dbg!(idx);
            };

            for event in entry {
                match event.into_parts() {
                    Either::Left(set_upstream) => {
                        self.set_remote_commit(&set_upstream).unwrap();
                    }
                    Either::Right(ItemAction { id, action }) => {
                        let evaluation =
                            match self.evaluate_action(id, action.clone(), false, false) {
                                Ok(eval) => eval,
                                Err(e) => {
                                    dbg!(e);
                                    dbg!(id, action);
                                    panic!();
                                }
                            };
                        self.apply_evaluation(evaluation.clone(), false).unwrap();

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

        dbg!(&modified, &dependents);

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
        if self.is_remote(key) {
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

    fn apply_evaluation(&self, res: ActionEvalResult<T>, cache: bool) -> Result<(), EventError<T>> {
        let ActionEvalResult {
            item,
            added_caches,
            removed_caches,
            is_no_op,
        } = res;

        if is_no_op {
            if !added_caches.is_empty() || !removed_caches.is_empty() {
                dbg!(&item, &added_caches, &removed_caches);
            }
        }

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
            }
            CardChange::Unchanged(_) => {}
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

    fn apply_and_save_upstream_commit(&self, event: SetUpstream) -> Result<(), EventError<T>> {
        self.set_remote_commit(&event)?;
        self.save_event(event);
        return Ok(());
    }

    fn save_event(&self, event: impl Into<LedgerEvent<T>>) {
        let hash = self.entries.save(event.into());
        self.set_ledger_hash(hash);
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
