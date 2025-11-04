use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::hash::Hash;
use std::ops::Deref;
use std::sync::{Arc, RwLock};

use crate::read_ledger::ReadLedger;
use crate::{Ledger, LedgerItem};

#[derive(Clone)]
pub struct Cache<T: LedgerItem> {
    pub items: Arc<RwLock<BTreeMap<T::Key, Arc<SavedItem<T>>>>>,
    pub ledger: Arc<Ledger<T>>,
}

impl<T: LedgerItem + Ord> Cache<T> {
    pub fn new(ledger: Arc<dyn ReadLedger<Item = T>>) -> Self {
        Self {
            items: Default::default(),
            ledger,
        }
    }

    pub fn insert_many(&self, items: impl IntoIterator<Item = Arc<SavedItem<T>>>) {
        let mut guard = self.items.write().unwrap();
        for item in items {
            guard.insert(item.key, item);
        }
    }

    pub fn insert(&self, item: Arc<SavedItem<T>>) {
        let mut guard = self.items.write().unwrap();
        guard.insert(item.item_id(), item);
    }

    /// When `key` has been modified, the cache will no longer be valid.
    ///
    /// We need to remove all its recursive dependents, as they may contain data relying on a previous state of `key`.
    /// Its dependencies however do not contain data relying on the previous state of `key`, as it's a DAG.
    /// However, the `SavedItem` wrapper lists its dependents, which may be invalid. So while the Item itself isn't invalid, the wrapper's dependents cache may be.
    pub fn invalidate(&self, key: T::Key) {
        let mut to_remove: BTreeSet<T::Key> = Default::default();

        let mut guard = self.items.write().unwrap();

        let Some(item) = guard.remove(&key) else {
            return;
        };

        drop(guard);

        let guard = self.items.read().unwrap();

        for dpy in item.recursive_dependents() {
            to_remove.insert(dpy);
        }

        for dpt in item.direct_dependencies() {
            to_remove.insert(dpt);
        }

        drop(guard);

        let mut guard = self.items.write().unwrap();

        for key in to_remove {
            guard.remove(&key);
        }
    }

    pub fn load_ids(&self) -> HashSet<T::Key> {
        self.items.read().unwrap().keys().cloned().collect()
    }

    pub fn load_all(&self) -> HashSet<Arc<SavedItem<T>>> {
        self.items.read().unwrap().values().cloned().collect()
    }

    pub fn load_cached(&self, key: T::Key) -> Option<Arc<SavedItem<T>>> {
        self.items.read().unwrap().get(&key).cloned()
    }

    pub fn load_or_fetch(&self, key: T::Key) -> Option<Arc<SavedItem<T>>> {
        if let Some(item) = self.items.read().unwrap().get(&key).map(Arc::clone) {
            return Some(item);
        }

        if let Some(item) = self.ledger.load_node(key, self.ledger.clone()) {
            let item = Arc::new(item);

            if let Ok(mut guard) = self.items.try_write() {
                guard.insert(key, Arc::clone(&item));
            }

            return Some(item);
        }

        None
    }
}

impl<T: LedgerItem> PartialEq for MaybeItem<T> {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}
impl<T: LedgerItem> Eq for MaybeItem<T> {}

impl<T: LedgerItem> PartialOrd for MaybeItem<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.key.partial_cmp(&other.key)
    }
}
impl<T: LedgerItem> Ord for MaybeItem<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.key.cmp(&other.key)
    }
}

/// Lazy loading for a [`SavedItem`].
pub struct MaybeItem<T: LedgerItem> {
    item: OnceCell<Arc<SavedItem<T>>>,
    key: T::Key,
    ledger: Arc<Ledger<T>>,
}

impl<T: LedgerItem> MaybeItem<T> {
    pub fn new(key: T::Key, ledger: Arc<Ledger<T>>) -> Self {
        Self {
            item: OnceCell::new(),
            key,
            ledger,
        }
    }

    fn id(&self) -> T::Key {
        self.key
    }
}

impl<T: LedgerItem + Ord> Deref for MaybeItem<T> {
    type Target = SavedItem<T>;

    fn deref(&self) -> &Self::Target {
        self.item
            .get_or_init(|| self.ledger.load_node_with_cache(self.key).unwrap())
    }
}

impl<T: LedgerItem> PartialEq for SavedItem<T> {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}
impl<T: LedgerItem> Eq for SavedItem<T> {}

impl<T: LedgerItem> PartialOrd for SavedItem<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.key.partial_cmp(&other.key)
    }
}
impl<T: LedgerItem> Ord for SavedItem<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.key.cmp(&other.key)
    }
}

impl<T: LedgerItem> Hash for SavedItem<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.key.hash(state);
    }
}

/// A wrapper around a [`LedgerItem`] for caching purposes.
pub struct SavedItem<T: LedgerItem> {
    pub item: OnceCell<T>,
    pub key: T::Key,
    pub dependencies: BTreeMap<T::RefType, BTreeSet<MaybeItem<T>>>,
    pub dependents: BTreeMap<T::RefType, BTreeSet<MaybeItem<T>>>,
    pub ledger: Arc<Ledger<T>>,
}

impl<T: LedgerItem + Ord> SavedItem<T> {
    pub fn id(&self) -> T::Key {
        self.key
    }

    pub fn clone_item(&self) -> T {
        self.item.get().cloned().unwrap()
    }

    pub fn direct_dependencies_with_ty(&self, ty: T::RefType) -> BTreeSet<T::Key> {
        self.dependencies
            .get(&ty)
            .map(|s| s.iter().map(|item| item.id()).collect())
            .unwrap_or_default()
    }

    pub fn direct_dependents_with_ty(&self, ty: T::RefType) -> BTreeSet<T::Key> {
        self.dependents
            .get(&ty)
            .map(|s| s.iter().map(|item| item.id()).collect())
            .unwrap_or_default()
    }

    pub fn direct_dependents(&self) -> BTreeSet<T::Key> {
        self.dependents
            .values()
            .flat_map(|s| s.iter().map(|item| item.id()))
            .collect()
    }

    pub fn direct_dependencies(&self) -> BTreeSet<T::Key> {
        self.dependencies
            .values()
            .flat_map(|s| s.iter().map(|item| item.id()))
            .collect()
    }

    pub fn recursive_dependencies_with_ty(&self, ty: T::RefType) -> BTreeSet<T::Key> {
        let mut dependencies: BTreeSet<T::Key> = BTreeSet::new();

        if let Some(set) = self.dependencies.get(&ty) {
            for dep in set {
                dependencies.insert(dep.id());
                dependencies.extend(dep.recursive_dependencies_with_ty(ty.clone()));
            }
        }

        dependencies
    }

    pub fn recursive_dependents_with_ty(&self, ty: T::RefType) -> BTreeSet<T::Key> {
        let mut dependents: BTreeSet<T::Key> = BTreeSet::new();

        if let Some(set) = self.dependents.get(&ty) {
            for dep in set {
                dependents.insert(dep.id());
                dependents.extend(dep.recursive_dependents_with_ty(ty.clone()));
            }
        }

        dependents
    }

    pub fn recursive_dependents(&self) -> BTreeSet<T::Key> {
        let mut dependents: BTreeSet<T::Key> = BTreeSet::new();

        for set in self.dependents.values() {
            for dep in set {
                dependents.insert(dep.id());
                dependents.extend(dep.recursive_dependents());
            }
        }

        dependents
    }

    pub fn recursive_dependencies(&self) -> BTreeSet<T::Key> {
        let mut dependencies: BTreeSet<T::Key> = BTreeSet::new();

        for set in self.dependencies.values() {
            for dep in set {
                dependencies.insert(dep.id());
                dependencies.extend(dep.recursive_dependencies());
            }
        }

        dependencies
    }
}

impl<T: LedgerItem> Deref for SavedItem<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.item
            .get_or_init(|| self.ledger.load(self.key).unwrap())
    }
}
