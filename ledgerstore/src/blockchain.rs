use std::{
    collections::BTreeMap,
    fs, io,
    path::Path,
    sync::{Arc, RwLock},
};

use tracing::info;

use crate::{
    entry_thing::{EntryNode, EventNode},
    DiskDirPath, Hashed, LedgerEntry, LedgerItem,
};

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
