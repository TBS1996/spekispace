pub mod fs;

pub type Hashed = String;
pub type Key = String;
pub type Item = String;

use std::{
    collections::{HashMap, HashSet},
    hash::{DefaultHasher, Hash, Hasher},
};

use serde::{de::DeserializeOwned, Serialize};
use tracing::trace;

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

        Self { hash, item }
    }
}

/// A key used to get all the items whose `property` matches `value`.
#[derive(Clone, Hash, Eq, PartialEq)]
pub struct PropertyCacheKey {
    pub property: String,
    pub value: String,
}

/// A key used to get all items who references current item with reference type `reftype`.
#[derive(Clone, Hash, Eq, PartialEq)]
pub struct RefCacheKey {
    pub reftype: String,
    pub id: String,
}

#[derive(Clone, Hash, Eq, PartialEq)]
pub enum CacheKey {
    Property(PropertyCacheKey),
    ItemRef(RefCacheKey),
}

impl CacheKey {
    fn to_string(&self) -> String {
        match self {
            Self::Property(PropertyCacheKey { property, value }) => format!("{property}{value}"),
            Self::ItemRef(RefCacheKey { reftype, id }) => format!("{reftype}{}", id),
        }
    }
}


use std::fmt::Debug;

/// Represents a single event in the ledger.
pub trait LedgerEvent:
    Hash + Debug + Clone + Serialize + DeserializeOwned + Send + Sync + 'static
{
    type Key: Copy + Eq + Hash + ToString + Debug;

    fn id(&self) -> Self::Key;

    fn data_hash(&self) -> Hashed {
        get_hash(self)
    }
}


/// Represents how a ledger mutates or creates an item.
pub trait LedgerItem<E: LedgerEvent + Debug>:
    Serialize + DeserializeOwned + Hash + 'static
{
    type Error: Debug;
    type RefType: AsRef<str>;
    type PropertyType: AsRef<str>;

    fn run_event(self, event: E) -> Result<Self, Self::Error>;

    fn new_default(id: E::Key) -> Self;

    fn item_id(&self) -> E::Key;

    /// List of references to other items, along with the name of the type of reference.
    /// 
    /// Used to create a index, like if item A references item B, we cache that item B is referenced by item A, 
    /// so that we don't need to search through all the items to find out or store it double in the item itself.
    fn ref_cache(&self) -> HashMap<Self::RefType, HashSet<E::Key>> {
        Default::default()
    }

    /// List of defined properties that this item has.
    /// 
    /// The property keys are predefined, hence theyre static str
    /// the String is the Value which could be anything. 
    /// For example ("suspended", true).
    fn properties_cache(&self) -> HashSet<(Self::PropertyType, String)> {
        Default::default()
    }

    fn caches(&self) -> HashSet<(CacheKey, String)> {
        trace!("fetching caches for item: {:?}", self.item_id());

        let mut out: HashSet<(CacheKey, String)> = Default::default();
        let id = self.item_id().to_string();

        for (property, value) in self.properties_cache()  {
            let key = PropertyCacheKey {
                property: property.as_ref().to_owned(),
                value,
            };
            out.insert((CacheKey::Property(key), id.clone()));
        }

        for (reftype, ids) in self.ref_cache() {
            let key = RefCacheKey {
                reftype: reftype.as_ref().to_owned(),
                id: id.to_string(),
            };

            for ref_id in ids {
                out.insert((CacheKey::ItemRef(key.clone()), ref_id.to_string()));
            }
        }

        out
    }
}

pub trait SnapStorage {
    fn save_on_gen(&self, key: &str, prev_generation: Option<&str>, item_hash: &str) -> Hashed;

    /// Saves item to the blob store
    fn save_item(&self, item_hash: &str, item: Vec<u8>);

    fn get_cache(&self, gen_hash: &str, cache_key: &CacheKey) -> Vec<String>;

    fn insert_cache(&self, gen_hash: &str, cache_key: &CacheKey, item: &str) -> Hashed;

    fn remove_cache(&self, gen_hash: &str, cache_key: &CacheKey, item: &str) -> Hashed;

    fn get_all(&self, gen_hash: &str) -> HashMap<String, Vec<u8>>;

    /// Saves the item and returns the hash to the new generation
    fn save(&self, gen_hash: Option<&str>, key: &str, item: Vec<u8>) -> Hashed {
        let item_hash = get_hash(&item);
        self.save_item(&item_hash, item);
        self.save_on_gen(key, gen_hash, &item_hash)
    }

    /// removes the item and returns the top hash
    fn remove(&self, gen_hash: &str, key: &str) -> Hashed;

    /// fetches an item
    fn get(&self, hash: &str, key: &str) -> Option<Vec<u8>>;
}
