//! hey there
//! Snapstore is a key-value database system where every change to the databsase creates an entirely new snapshot of the
//! state while minimizing space on disk.

pub mod fs;
//pub mod mem;

/*

lets seee

so when we save an item, we just care about like hte path to the item inthe leafdir bsaically
but with a cache, like the item is the actual folder itself, yaknow?
how can we generalize these two things so that both htings can call the same thing?
hmm when we save a single item we can say that we "create" a new leafdir in a way.
so when we save an item, we can fetch the leafdir that we woant to modify, then modify it
then like return a more high level function this new leafdir, then save it in the same way that we would save a cache dir

maybe we should have two diff directories, one for just pure itemhash -> item
and the other for those paths to that item, to more clearly separate the two concepts

maybe even split it in 4

# 1. state hashes, like maps of state hash to the first key component dir.
# 2. key component dirs. so these are the dirs where each entry name is a key component, and it points to either a leafdir or another keycomponent dir.
# 3. leafdirs, a bunch of dirs that contain items. itemkey -> item. hardlinks to a given itemhash.
# 4. blobstore, just a bunch of normal files that are itemhash -> item

could make it even more structured and have a separate entry for each level on the key component thing
i guess the only advantage is debugging? or maybe it would be easier to dynamically add more key components later on? idk


anyway, to generalize over saving a cache leafdir thing and a simple item, we can do like this

for saving item:

1. save item to blobstore
2. retrieve the leafdir where this item will reside (create new empty if not existing)
3. modify it, by inserting the new item, which is just a hardlink to the item with a given hash to the blobstore
4. save leafdir, will give you back a path to it and a new statehash which you'll save in the state hashes thing

for saving a cachedir:

1. directly create a leafdir which contains all the items you wanna cache under the given cachekey.
2. save leafdir in same manner. but here the key is the cachekey. you can then create new statehash in same manner.

for retrieving...

so to maximize generality we would have a function that just get the leafdir, but when we want a simple item we wanna just go to the path directly lol



hmm wait, so, the thing with the key is that, it has like two parts in a way. wait actually 3

1. the state hash, which represents the top level dir
2. the key components, which represents the path to a leafdir
3. the key id, which, which represents the way to get the key from the given leafdir

so you can imagine the leafdir key is just the state hash and the components
the itemstorekey is the leafdir + item id.

right so i'd want to still keep the cache separately

i think the stuff i need for cache is more basic than the save item stuff

like i'd have a base trait/utility that does the things i need for cachedirs, where you deal with fetching leafdir and modifying it and saving it
then i'd have a supertrait for the saving items which just simplifies the logic for saving and retrieving single items.

then some object that encapsulates both the caching and item retrieval which internally contains the cachething and the itemkey thing.


*/

pub type Hashed = String;
pub type Key = String;
pub type Item = String;
pub type CacheHash = Hashed;

/// The return type when modifying the snapstore. You get the new top hash of the modified store, plus a list of all added paths.
/// The added paths used for garbage collection.
pub type HashAndContents = (Hashed, Vec<Content>);

use std::fmt::Debug;
use std::path::PathBuf;
use std::sync::Arc;
use std::{
    collections::{HashMap, HashSet},
    hash::{DefaultHasher, Hash, Hasher},
};

use fs::Content;

/// A key used to get all the items whose `property` matches `value`.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct PropertyCacheKey {
    pub property: String,
    pub value: String,
}

/// A key used to get all items who references current item with reference type `reftype`.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct RefCacheKey {
    pub reftype: String,
    pub id: String,
}

#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub enum CacheKey {
    Property(PropertyCacheKey),
    ItemRef(RefCacheKey),
}

impl CacheKey {
    pub fn to_string(&self) -> String {
        match self {
            Self::Property(PropertyCacheKey { property, value }) => format!("{property}:{value}"),
            Self::ItemRef(RefCacheKey { reftype, id }) => format!("{reftype}:{id}"),
        }
    }
}

/// The information needed to locate a leaf directory.
pub struct LeafKey {
    state: String,
    components: Vec<char>,
}

pub struct KeyFoo<'a> {
    key: &'a str,
    cmps: Vec<char>,
}

/// Main trait lol
/// hmm maybe instead of loading two snapshots and checking all the stuff between that's not used for garbage collection
/// then each time we save a new item, if it's trivial to check if the new unused contents don't have any owner then store those
/// directly and remove them. like, we save an item, then the new contents A, B, C is created
/// then we store a new one and the contents D E F created. if we can tell easily which of A B C no longer used, then we know it's safe to delete
/// them later. since they're inbetween snapshots.
///
/// hmm mayve if we have in-memory all the new stuff created from last snapshot, when we create a new itempath we can check if the hashes of the
/// discarded ones belong in the new-stuff. if they do, we can put them in a different file
/// so at the point of the next snapshot, we'll have two files. one representing all the new contents of the snapshot
/// the other representing hte things that were created along the way to the new snapshot but no longer required, which can be safely cleaned up.
/// then there's no need to traverse the entire store.
///
/// for caching, i think remove we the caching methods here and have a new trait for that which has this as supertrait.
pub trait SnapStorage {
    /// Creates like a new generation that includes the new item. must be called after [`Self::save_item`].
    fn save_on_gen(
        &self,
        key: KeyFoo<'_>,
        prev_generation: Option<&str>,
        item_hash: &str,
    ) -> HashAndContents;

    /// Saves item to the blob store. Does not update any paths to the item.
    fn save_item(&self, item_hash: &str, item: Vec<u8>);

    fn get_all(&self, gen_hash: &str) -> HashMap<String, Vec<u8>>;

    fn all_paths(&self, _gen_hashh: &str) -> HashSet<Content> {
        todo!()
    }

    /// removes the item and returns the top hash
    fn remove(&self, gen_hash: &str, key: &str) -> HashAndContents;

    /// fetches an item
    fn get(&self, hash: &str, key: &str) -> Option<Vec<u8>>;

    /// Saves the item and returns the hash to the new generation and list of added paths.
    fn save(&self, gen_hash: Option<&str>, key: &str, item: Vec<u8>) -> HashAndContents {
        let key = KeyFoo {
            key,
            cmps: self.get_key_components(key),
        };
        let item_hash = get_hash(&item);
        self.save_item(&item_hash, item);
        self.save_on_gen(key, gen_hash, &item_hash)
    }

    fn num_key_components(&self) -> usize {
        3
    }

    fn get_key_components(&self, key: &str) -> Vec<char> {
        let hash = get_hash(&key.to_string());
        let mut out = vec![];
        let mut chars = hash.chars();

        for _ in 0..self.num_key_components() {
            out.push(chars.next().unwrap());
        }

        tracing::trace!("key: {key}, components: {out:?}");

        out
    }
}

pub(crate) fn get_hash<T: Hash>(item: &T) -> Hashed {
    let mut hasher = DefaultHasher::new();
    item.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

/// Hmm do i even need to do that thing where i get an itempath that recursively checks if the path exist at every point
/// and then create it o nthe fly if not exist when fetching?
/// can't i just directly see if the leafdir exist, if not, create it, and save each path to it, if it already exist i just don't save it

pub struct LeafdirStorage {
    blob: Arc<PathBuf>,
}

impl LeafdirStorage {
    /// so uh this has to exist i guess, so should be used when fetching an item lol
    fn leafdir_path(&self, key: LeafKey) -> PathBuf {
        todo!()
    }

    /// so when you have some new leafdir to save, this is it lol.
    /// so let's seee...
    /// i guess we have to start on the end, save the content as a new leafdir, where the dir name is the hash of its contents
    fn save_leafdir(
        &self,
        prev_hash: Option<&str>,
        cmps: Vec<char>,
        contents: Vec<(String, PathBuf)>,
    ) -> LeafKey {
        todo!()
    }
}
