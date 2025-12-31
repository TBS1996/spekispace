use std::{
    collections::BTreeMap,
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    io,
    path::Path,
    sync::{Arc, RwLock},
};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tracing::info;

use crate::{
    entry_thing::{EntryNode, EventNode},
    DiskDirPath, Hashed, ItemExpr, LedgerItem,
};

#[derive(Clone, Serialize, Deserialize, Debug, Hash)]
#[serde(bound(deserialize = "T: LedgerItem + DeserializeOwned,
                   T::Key: DeserializeOwned"))]
pub struct LedgerEntry<T: LedgerItem> {
    pub previous: Option<Hashed>,
    pub index: usize,
    pub event: LedgerEvent<T>,
}

impl<T: LedgerItem> LedgerEntry<T> {
    pub(crate) fn new(previous: Option<&Self>, event: LedgerEvent<T>) -> Self {
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

    pub(crate) fn data_hash(&self) -> Hashed {
        get_hash(self)
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
    ItemAction {
        id: T::Key,
        action: LedgerAction<T>,
    },
    SetUpstream {
        commit: String,
        upstream_url: String,
    },
    DeleteSet {
        set: ItemExpr<T>,
    },
}

pub(crate) struct ItemAction<T: LedgerItem> {
    pub(crate) id: T::Key,
    pub(crate) action: LedgerAction<T>,
}

#[derive(Clone, Debug)]
pub(crate) struct SetUpstream {
    pub(crate) commit: String,
    pub(crate) upstream_url: String,
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
    pub fn new(id: T::Key, action: LedgerAction<T>) -> Self {
        Self::ItemAction { id, action }
    }

    pub fn id(&self) -> Option<T::Key> {
        match self {
            LedgerEvent::ItemAction { id, .. } => Some(*id),
            LedgerEvent::SetUpstream { .. } => None,
            LedgerEvent::DeleteSet { .. } => None,
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

fn get_hash<T: Hash>(item: &T) -> Hashed {
    let mut hasher = DefaultHasher::new();
    item.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

pub fn max_dir_number(path: impl AsRef<Path>) -> io::Result<Option<usize>> {
    let max = fs::read_dir(path)?
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().to_str()?.parse::<usize>().ok())
        .max();
    Ok(max)
}

#[derive(Clone, Debug)]
pub struct BlockChain<T: LedgerItem> {
    cached: Arc<RwLock<Option<BTreeMap<usize, EntryNode<T>>>>>,
    entries_path: Arc<DiskDirPath>,
}

impl<T: LedgerItem> BlockChain<T> {
    pub fn new(path: DiskDirPath) -> Self {
        Self {
            cached: Arc::new(RwLock::new(None)),
            entries_path: Arc::new(path),
        }
    }

    pub fn chain(&self) -> BTreeMap<usize, EntryNode<T>> {
        info!("fetching chain");
        if self.cached.read().unwrap().is_some() {
            return self.cached.read().unwrap().clone().unwrap();
        }

        let ledger = Self::load_ledger(&self.entries_path);
        *self.cached.write().unwrap() = Some(ledger.clone());
        ledger
    }

    pub fn current_hash(&self) -> Option<Hashed> {
        self.current_head().map(|entry| entry.data_hash())
    }

    fn highest_index(&self) -> Option<usize> {
        match self.cached.read().unwrap().as_ref() {
            Some(cached) => cached.iter().next_back().map(|(idx, _)| idx).cloned(),
            None => max_dir_number(self.entries_path.as_path()).unwrap(),
        }
    }

    /// The index of the next entry to be added.
    fn working_index(&self) -> usize {
        self.highest_index().map(|num| num + 1).unwrap_or(0)
    }

    fn current_head(&self) -> Option<LedgerEntry<T>> {
        if let Some(chain) = self.cached.read().unwrap().as_ref() {
            if let Some((_, entry)) = chain.iter().next_back() {
                return Some(entry.last().clone());
            }
        }

        let mut idx = self.working_index();

        if idx == 0 {
            return None;
        }

        idx -= 1;

        EntryNode::load_single(&**self.entries_path, idx).map(|x| x.last().to_owned())
    }

    pub fn save_entry(&self, entry: EventNode<T>) -> Hashed {
        let idx = self.working_index();

        let prev = self.current_head();

        let entry = entry.clone().save(&self.entries_path, idx, prev);

        let hash = entry.data_hash();

        self.cached
            .write()
            .unwrap()
            .get_or_insert_default()
            .insert(idx, entry);

        hash
    }

    fn load_ledger(space: &Path) -> BTreeMap<usize, EntryNode<T>> {
        info!("loading entire ledger to memory");
        EntryNode::load_chain(space)
    }
}
