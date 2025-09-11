use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use tracing::info;

use crate::{load_file_contents, Hashed, LedgerEntry, LedgerEvent, LedgerItem};

#[derive(Clone)]
pub struct BlockChain<T: LedgerItem> {
    cached: Arc<RwLock<Option<Vec<LedgerEntry<T>>>>>,
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

    pub fn chain(&self) -> Vec<LedgerEntry<T>> {
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

    fn current_index(&self) -> usize {
        if self.cached.read().unwrap().is_some() {
            self.cached.read().unwrap().as_ref().unwrap().len()
        } else {
            std::fs::read_dir(self.entries_path.as_path())
                .unwrap()
                .count()
        }
    }

    fn current_head(&self) -> Option<LedgerEntry<T>> {
        if let Some(chain) = self.cached.read().unwrap().as_ref() {
            return chain.last().cloned();
        }

        let idx = self.current_index();

        if idx == 0 {
            return None;
        }

        let name = format!("{:06}", idx - 1);
        let path = self.entries_path.join(name);

        let action: LedgerEntry<T> = match serde_json::from_str(&fs::read_to_string(&path).unwrap())
        {
            Ok(action) => action,
            Err(e) => {
                dbg!(e);
                panic!();
            }
        };

        Some(action)
    }

    pub fn save(&self, event: LedgerEvent<T>) -> Hashed {
        use std::io::Write;

        let previous = self.current_head();
        let entry = LedgerEntry::new(previous.as_ref(), event);

        let name = format!("{:06}", self.current_index());
        let path = &self.entries_path.join(name);
        assert!(!path.exists());
        let mut file = std::fs::File::create_new(path).unwrap();

        let serialized = serde_json::to_string_pretty(&entry).unwrap();
        file.write_all(serialized.as_bytes()).unwrap();

        if let Some(vec) = self.cached.write().unwrap().as_mut() {
            vec.push(entry);
        }

        self.current_hash().unwrap()
    }

    fn load_ledger(space: &Path) -> Vec<LedgerEntry<T>> {
        info!("loading entire ledger to memory");

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
