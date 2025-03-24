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

struct HashedItem<T: Hash> {
    hash: String,
    item: T,
}

impl<T: Hash> HashedItem<T> {
    fn new(item: T) -> Self {
        let hash = get_hash(&item);

        Self {
            hash,
            item,
        }
    }
}

pub trait SnapStorage {
    fn save_on_gen(&self, key: &str, prev_generation: Option<&str>, item_hash: &str) -> Hashed; 

    /// Saves item to the blob store
    fn save_item(&self, item_hash: &str, item: Vec<u8>);

    fn get(&self, hash: &str, key: &str) -> Option<Vec<u8>>;

    fn get_cache(&self, gen_hash: &str, cache_key: &str) -> Vec<String>;

    fn insert_cache(&self, gen_hash: &str, cache_key: &str, item: &str) -> Hashed;

    fn remove_cache(&self, gen_hash: &str, cache_key: &str, item: &str) -> Hashed;

    fn get_all(&self, gen_hash: &str) -> HashMap<String, Vec<u8>>;

    /// Saves the item and returns the hash to the new generation
    fn save(&self, gen_hash: Option<&str>, key: &str, item: Vec<u8>) -> Hashed{
        let item_hash = get_hash(&item);
        self.save_item(&item_hash, item);
        self.save_on_gen(key, gen_hash, &item_hash)
    }

    /// removes the item and returns the top hash
    fn remove(&self, gen_hash: &str, key: &str) -> Hashed;
}