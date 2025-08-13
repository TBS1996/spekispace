use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use crate::{load_file_contents, Hashed, LedgerEntry, LedgerEvent, LedgerHash, LedgerItem};

#[derive(Clone)]
pub struct BlockChain<T: LedgerItem> {
    cached: Arc<RwLock<Vec<LedgerEntry<T>>>>,
    index_to_hash: Arc<RwLock<HashMap<LedgerHash, usize>>>,
    entries_path: Arc<PathBuf>,
}

impl<T: LedgerItem> BlockChain<T> {
    pub fn new(path: PathBuf) -> Self {
        std::fs::create_dir_all(&path).unwrap();
        let cached = Self::load_ledger(&path);
        let mut index_to_hash: HashMap<LedgerHash, usize> = Default::default();

        for entry in &cached {
            index_to_hash.insert(entry.data_hash(), entry.index);
        }

        Self {
            cached: Arc::new(RwLock::new(cached)),
            index_to_hash: Arc::new(RwLock::new(index_to_hash)),
            entries_path: Arc::new(path),
        }
    }

    pub fn chain(&self) -> Vec<LedgerEntry<T>> {
        self.cached.read().unwrap().clone()
    }

    pub fn current_hash(&self) -> Option<Hashed> {
        self.current_head().map(|entry| entry.data_hash())
    }

    fn current_index(&self) -> usize {
        self.cached.read().unwrap().len()
    }

    fn current_head(&self) -> Option<LedgerEntry<T>> {
        self.cached.read().unwrap().last().cloned()
    }

    pub fn save(&self, event: LedgerEvent<T>) -> Hashed {
        use std::io::Write;

        let previous = self.current_head();
        let entry = LedgerEntry::new(previous.as_ref(), event);
        let index = self.current_index();
        let ledger_hash = entry.data_hash();

        let name = format!("{:06}", self.current_index());
        let path = &self.entries_path.join(name);
        assert!(!path.exists());
        let mut file = std::fs::File::create_new(path).unwrap();

        let serialized = serde_json::to_string_pretty(&entry).unwrap();
        file.write_all(serialized.as_bytes()).unwrap();
        self.cached.write().unwrap().push(entry);
        self.index_to_hash
            .write()
            .unwrap()
            .insert(ledger_hash.clone(), index);

        self.current_hash().unwrap()
    }

    fn load_ledger(space: &Path) -> Vec<LedgerEntry<T>> {
        let mut foo: Vec<(usize, LedgerEntry<T>)> = {
            let map: HashMap<String, Vec<u8>> = load_file_contents(space);
            let mut foo: Vec<(usize, LedgerEntry<T>)> = Default::default();

            if map.is_empty() {
                return vec![];
            }

            for (_hash, value) in map.into_iter() {
                let action: LedgerEntry<T> = match serde_json::from_slice(&value) {
                    Ok(action) => action,
                    Err(e) => {
                        dbg!(e);
                        dbg!(value);
                        panic!();
                    }
                };
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
}
