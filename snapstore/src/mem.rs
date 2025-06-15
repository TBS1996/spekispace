use std::{
    collections::{BTreeMap, HashMap, HashSet},
    marker::PhantomData,
    path::PathBuf,
    str::FromStr,
};

use std::hash::Hash;

use crate::{
    Hashed, Key,
    fs::{Dir, FsDir, SnapFs, get_key_components},
    get_hash,
};

type Item = Vec<u8>;
type ItemHash = Hashed;

#[derive(Default, Clone, PartialEq, Ord, PartialOrd, Eq)]
struct Topdir([Option<MiddleDir>; 16]);
#[derive(Default, Clone, PartialEq, Ord, PartialOrd, Eq)]
struct MiddleDir([Option<LeafDir>; 16]);

#[derive(Default, Clone, PartialEq, Ord, PartialOrd, Eq)]
struct LeafDir([Option<ItemDir>; 16]);

#[derive(Default, Clone, PartialEq, Ord, PartialOrd, Eq)]
struct ItemDir(BTreeMap<Key, ItemHash>);

#[derive(Clone)]
pub struct SnapMem<K>
where
    K: Copy + Eq + FromStr + ToString + Hash, // + Hash + Debug + Serialize + DeserializeOwned + Send + Sync,
{
    items: HashMap<ItemHash, Item>,
    top_dir: Topdir,
    _phantom: PhantomData<K>,
}

impl<K: Copy + Eq + FromStr + ToString + Hash> Default for SnapMem<K> {
    fn default() -> Self {
        Self {
            items: Default::default(),
            top_dir: Default::default(),
            _phantom: Default::default(),
        }
    }
}

fn hex_char_to_value(c: char) -> Option<usize> {
    c.to_digit(16).map(|v| v as usize)
}

fn value_to_hex_char(val: u8) -> char {
    match val {
        0..=9 => (b'0' + val) as char,
        10..=15 => (b'a' + (val - 10)) as char,
        _ => panic!("invalid nibble: {}", val),
    }
}

fn key_cmps(key: &str) -> [usize; 3] {
    get_key_components(3, key)
        .into_iter()
        .map(|c| hex_char_to_value(c).unwrap())
        .collect::<Vec<_>>()
        .try_into()
        .unwrap()
}

impl<K: Copy + Eq + FromStr + ToString + Hash> SnapMem<K> {
    pub fn persist(mut self, fs: SnapFs<K>) -> (Hashed, HashMap<ItemHash, Item>) {
        let topdir = self.top_dir;
        let middle_dirs: Vec<MiddleDir> = topdir.0.clone().into_iter().filter_map(|x| x).collect();
        let leaf_dirs: Vec<LeafDir> = middle_dirs
            .clone()
            .into_iter()
            .flat_map(|m| m.0.into_iter().flatten())
            .collect();
        let item_dirs: Vec<ItemDir> = leaf_dirs
            .clone()
            .into_iter()
            .flat_map(|m| m.0.into_iter().flatten())
            .collect();

        let final_item_hashes: HashSet<&ItemHash> =
            item_dirs.iter().map(|x| x.0.values()).flatten().collect();

        self.items.retain(|key, _| final_item_hashes.contains(key));

        let mut items: HashMap<ItemHash, PathBuf> = Default::default();
        for (hash, item) in &self.items {
            let (_, path) = fs.save_item(&hash, item.clone());
            items.insert(hash.to_string(), path);
        }

        let mut item_dir_map: BTreeMap<ItemDir, FsDir> = Default::default();

        for itemdir in item_dirs {
            let mut thedir = Dir::new();
            for (key, item) in itemdir.0.clone() {
                let path = items.get(&item).unwrap().clone();
                thedir.insert_file(key, path);
            }
            let (fsdir, _) = thedir.persist(fs.blob_path.clone(), &mut vec![]);
            item_dir_map.insert(itemdir, fsdir);
        }

        let mut leaf_dir_map: BTreeMap<LeafDir, FsDir> = Default::default();

        for leaf_dir in leaf_dirs {
            let mut thedir = Dir::new();

            for (index, itemdir) in leaf_dir.0.clone().into_iter().enumerate() {
                if let Some(itemdir) = itemdir {
                    let path = item_dir_map.get(&itemdir).unwrap().path();
                    let ch = value_to_hex_char(index as u8);
                    thedir.insert_dir(ch.to_string(), path);
                }
            }

            let (fsdir, _) = thedir.persist(fs.blob_path.clone(), &mut vec![]);
            leaf_dir_map.insert(leaf_dir, fsdir);
        }

        let mut mid_dir_map: BTreeMap<MiddleDir, FsDir> = Default::default();

        for mid_dir in middle_dirs {
            let mut thedir = Dir::new();

            for (index, leaf_dir) in mid_dir.0.clone().into_iter().enumerate() {
                if let Some(leaf_dir) = leaf_dir {
                    let path = leaf_dir_map.get(&leaf_dir).unwrap().path();
                    let ch = value_to_hex_char(index as u8);
                    thedir.insert_dir(ch.to_string(), path);
                }
            }

            let (fsdir, _) = thedir.persist(fs.blob_path.clone(), &mut vec![]);
            mid_dir_map.insert(mid_dir, fsdir);
        }

        let mut thedir = Dir::new();

        for (index, mid_dir) in topdir.0.clone().into_iter().enumerate() {
            if let Some(mid_dir) = mid_dir {
                let path = mid_dir_map.get(&mid_dir).unwrap().path();
                let ch = value_to_hex_char(index as u8);
                thedir.insert_dir(ch.to_string(), path);
            }
        }

        let (fsdir, _) = thedir.persist(fs.blob_path.clone(), &mut vec![]);

        (fsdir.hash(), self.items)
    }

    /// Saves the item and returns the hash to the new generation and list of added paths.
    pub fn save(&mut self, key: &str, item: Vec<u8>) {
        let item_hash = get_hash(&item);
        self.items.insert(item_hash.to_string(), item);
        let itemdir = self.itemdir_mut(key);
        itemdir.0.insert(key.to_string(), item_hash);
    }

    fn itemdir_mut(&mut self, key: &str) -> &mut ItemDir {
        let cmps = key_cmps(key);
        let middle = self
            .top_dir
            .0
            .get_mut(cmps[0])
            .unwrap()
            .get_or_insert_default();
        let leaf = middle.0.get_mut(cmps[1]).unwrap().get_or_insert_default();
        leaf.0.get_mut(cmps[2]).unwrap().get_or_insert_default()
    }

    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        let cmps = key_cmps(key);
        let mid = self.top_dir.0[cmps[0] as usize].as_ref()?;
        let leaf = mid.0[cmps[1] as usize].as_ref()?;
        let item_dir = leaf.0[cmps[2] as usize].as_ref()?;

        let item_hash = item_dir.0.get(key)?;
        let item = self.items.get(item_hash)?;

        Some(item.clone())
    }

    pub fn remove(&mut self, key: &str) {
        self.itemdir_mut(key).0.remove(key).unwrap();
    }
}
