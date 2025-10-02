use std::{
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use tracing::info;

use crate::{entry_thing::EntryThing, Hashed, LedgerEvent, LedgerItem};

#[derive(Clone, Debug)]
pub struct BlockChain<T: LedgerItem> {
    cached: Arc<RwLock<Option<Vec<EntryThing<T>>>>>,
    entries_path: Arc<PathBuf>,
}

impl<T: LedgerItem> BlockChain<T> {
    pub fn new(path: PathBuf) -> Self {
        std::fs::create_dir_all(&path).unwrap();

        Self {
            cached: Arc::new(RwLock::new(None)),
            entries_path: Arc::new(path),
        }
    }

    pub fn chain(&self) -> Vec<EntryThing<T>> {
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

    fn working_index(&self) -> usize {
        if self.cached.read().unwrap().is_some() {
            self.cached.read().unwrap().as_ref().unwrap().len()
        } else {
            std::fs::read_dir(self.entries_path.as_path())
                .unwrap()
                .count()
        }
    }

    fn current_head(&self) -> Option<LedgerEvent<T>> {
        if let Some(chain) = self.cached.read().unwrap().as_ref() {
            if let Some(entry) = chain.last() {
                return Some(entry.last_entry().clone());
            }
        }

        let mut idx = self.working_index();

        if idx == 0 {
            return None;
        }

        idx -= 1;

        EntryThing::load_single(&*self.entries_path, idx).map(|x| x.last_entry().to_owned())
    }

    pub fn save_entry(&self, entry: EntryThing<T>) -> Hashed {
        let idx = self.working_index();

        let hash = entry
            .clone()
            .save(&self.entries_path, idx)
            .last()
            .unwrap()
            .data_hash();

        self.cached
            .write()
            .unwrap()
            .get_or_insert_default()
            .push(entry);

        hash
    }

    pub fn save(&self, event: LedgerEvent<T>) -> Hashed {
        let entry = EntryThing::Leaf(event);
        let idx = self.working_index();

        let hash = entry
            .clone()
            .save(&self.entries_path, idx)
            .last()
            .unwrap()
            .data_hash();

        self.cached
            .write()
            .unwrap()
            .get_or_insert_default()
            .push(entry);

        hash
    }

    fn load_ledger(space: &Path) -> Vec<EntryThing<T>> {
        info!("loading entire ledger to memory");
        EntryThing::load_chain(space)
    }
}
