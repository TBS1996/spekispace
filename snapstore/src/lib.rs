pub mod fs;

pub type Hashed = String;
pub type Key = String;
pub type Item = String;
 
use std::{collections::HashMap, hash::{DefaultHasher, Hash, Hasher}};

fn get_hash<T: Hash>(item: &T) -> Hashed {
    let mut hasher = DefaultHasher::new();
    item.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

pub trait SnapStorage {
    fn save_on_gen(&self, key: &str, prev_generation: &str, item_hash: &str) -> Hashed; 

    fn save_item(&self, item_hash: &str, item: Vec<u8>);

    fn get_with_gen(&self, key: &str, gen_hash: &Hashed) -> Option<Vec<u8>>;

    fn get_gen(&self, idx: usize) -> Hashed;

    fn latest_generation(&self) -> Hashed;

    fn get_with_gen_idx(&self, key: &Key, generation: usize) -> Option<Vec<u8>> {
        let hash = self.get_gen(generation);
        self.get_with_gen(key, &hash)
    }

    fn get(&self, key: &str) -> Option<Vec<u8>> {
        let hash = self.latest_generation();
        self.get_with_gen(key, &hash)
    }

    fn get_all(&self) -> HashMap<String, Vec<u8>>;

    fn save(&self, key: &str, item: Vec<u8>) -> Hashed{
        let hash = self.latest_generation();
        let item_hash = get_hash(&item);
        self.save_item(&item_hash, item);
        self.save_on_gen(key, &hash, &item_hash)
    }
}