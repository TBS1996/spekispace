use std::{collections::HashMap, path::PathBuf};

use crate::{Hashed, Key, SnapStorage};

type KeyComponent = char;
type Item = Vec<u8>;
type ItemHash = Hashed;

struct LeafDir(HashMap<Key, ItemHash>);

struct InternalDir(HashMap<KeyComponent, Dir>);

enum Dir {
    Leaf(LeafDir),
    Int(InternalDir),
}

enum Content {
    Dir(Dir),
    Item(Item),
}

pub struct SnapMem {
    blobs: HashMap<Hashed, Content>,
    states: Vec<Vec<Vec<LeafDir>>>,
}

impl SnapStorage for SnapMem {
    fn save_on_gen(
        &self,
        key: &str,
        prev_generation: Option<&str>,
        item_hash: &str,
    ) -> crate::HashAndContents {
        todo!()
    }

    fn save_item(&self, item_hash: &str, item: Vec<u8>) {
        todo!()
    }

    fn get_all(&self, gen_hash: &str) -> HashMap<String, Vec<u8>> {
        todo!()
    }

    fn remove(&self, gen_hash: &str, key: &str) -> crate::HashAndContents {
        todo!()
    }

    fn get(&self, hash: &str, key: &str) -> Option<Vec<u8>> {
        todo!()
    }
}
