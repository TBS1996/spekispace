use std::{
    collections::HashMap,
    fs::{self, read_link},
    hash::{DefaultHasher, Hash, Hasher},
    ops::Deref,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    sync::Arc,
};

use tracing::trace;
use walkdir::WalkDir;

use crate::CacheKey;

use super::SnapStorage;

type Hashed = String;

#[derive(Debug, Clone)]
pub struct SnapFs {
    blob_path: Arc<PathBuf>,
}

impl SnapStorage for SnapFs {
    fn save_on_gen(
        &self,
        key: &str,
        prev_generation: Option<&str>,
        item_hash: &str,
    ) -> crate::Hashed {
        let item_path = full_blob_path(&self.blob_path, item_hash);

        let top_map = match prev_generation {
            Some(prev_gen) => FsDir::load(self.blob_path.clone(), prev_gen.to_owned()).unwrap(),
            None => {
                let empty = Dir::new();
                empty.persist(self.blob_path.clone())
            }
        };

        let all: ItemPath = top_map.all_dirs(&key);
        let x = all.save_item(key.to_owned(), item_path);
        let top_hash = x.first().unwrap().hash.clone();
        top_hash
    }

    fn get_all(&self, hash: &str) -> HashMap<String, Vec<u8>> {
        let path = self.the_full_blob_path(hash);
        let mut file_map = HashMap::new();

        for entry in WalkDir::new(&path) {
            let entry = entry.unwrap();
            let path = entry.path();

            if path.is_file() {
                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    let contents = fs::read(path).unwrap();
                    file_map.insert(file_name.to_string(), contents);
                }
            }
        }

        file_map
    }

    fn get_cache(&self, gen_hash: &str, cache_key: &CacheKey) -> Vec<String> {
        let path = self.full_path(gen_hash, &cache_key.to_string());

        let mut out = vec![];
        for entry in fs::read_dir(&path).unwrap() {
            out.push(entry.unwrap().file_name().into_string().unwrap());
        }

        out
    }

    fn insert_cache(&self, gen_hash: &str, cache_key: &CacheKey, item: &str) -> Hashed {
        let item_blob_path = self.get_item_path(gen_hash, item);
        let path = FsDir::load(self.blob_path.clone(), gen_hash.to_owned()).unwrap();
        let itempath = path.all_dirs(&cache_key.to_string());
        itempath
            .save_item(item.to_string(), item_blob_path)
            .first()
            .unwrap()
            .get_hash()
    }

    fn remove_cache(&self, gen_hash: &str, cache_key: &CacheKey, item: &str) -> Hashed {
        let topdir = FsDir::load(self.blob_path.clone(), gen_hash.to_string()).unwrap();
        let all = topdir.all_dirs(&cache_key.to_string());
        all.remove_item(item.to_owned()).first().unwrap().get_hash()
    }

    fn get(&self, hash: &str, key: &str) -> Option<Vec<u8>> {
        let path = self.full_path(hash, key);
        fs::read(&path).ok()
    }

    fn save_item(&self, item_hash: &str, item: Vec<u8>) {
        let path = self.the_full_blob_path(item_hash);
        fs::File::create(&path).unwrap();
        fs::write(&path, &item).unwrap();
    }

    fn remove(&self, gen_hash: &str, key: &str) -> crate::Hashed {
        let topdir = FsDir::load(self.blob_path.clone(), gen_hash.to_string()).unwrap();
        let all = topdir.all_dirs(key);
        all.remove_item(key.to_owned()).first().unwrap().get_hash()
    }
}

impl SnapFs {
    pub fn new(root: PathBuf) -> Self {
        let blob_path = Arc::new(root.join("blobs"));
        fs::create_dir_all(&*blob_path).unwrap();

        let selv = Self { blob_path };

        selv
    }

    fn get_item_path(&self, genn: &str, item_key: &str) -> PathBuf {
        let itemhash = self.get_item_hash(genn, item_key);
        self.the_full_blob_path(&itemhash)
    }

    fn get_item_hash(&self, genn: &str, item: &str) -> Hashed {
        read_link(self.full_path(genn, item))
            .unwrap()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned()
    }

    pub fn the_full_blob_path(&self, hash: &str) -> PathBuf {
        full_blob_path(&self.blob_path, hash)
    }

    fn full_path(&self, _gen: &str, key: &str) -> PathBuf {
        let mut path = self.the_full_blob_path(_gen);
        for cmp in get_key_components(key) {
            path = path.join(format!("{cmp}"));
        }

        path = path.join(key);

        path
    }
}

fn get_hash<T: Hash>(item: &T) -> Hashed {
    let mut hasher = DefaultHasher::new();
    item.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

/// A Dir as it exists on the filesystem, cannot be mutated inmemory, only acquired by loading from fs.
#[derive(Debug)]
struct FsDir {
    blob_path: Arc<PathBuf>,
    hash: Hashed,
    dir: Dir,
}

impl Deref for FsDir {
    type Target = Dir;

    fn deref(&self) -> &Self::Target {
        &self.dir
    }
}

impl FsDir {
    fn load(dir_path: Arc<PathBuf>, hash: Hashed) -> Option<Self> {
        let path = full_blob_path(&dir_path, &hash);
        if !path.exists() {
            return None;
        }

        let mut contents: HashMap<String, Content> = Default::default();

        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name().into_string().unwrap();
            let path = entry.path();
            let sympath = match fs::read_link(&path) {
                Ok(p) => p,
                Err(_e) => path,
            };
            contents.insert(name, Content::new(sympath));
        }

        let dir = Dir { contents };

        Some(Self {
            blob_path: dir_path,
            hash,
            dir,
        })
    }

    fn into_inner(self) -> Dir {
        self.dir
    }

    fn path(&self) -> PathBuf {
        full_blob_path(&self.blob_path, &self.hash)
    }

    /// must only be called from top-level directory
    ///
    /// returns a vector of all the directories from current to the one where the item of a given key is located.
    /// if the key doesn't exist it'll fill it with empty dir objects.
    fn all_dirs(self, key: &str) -> ItemPath {
        trace!("retrieving full itempath of key: {key}");
        let dir_path = self.blob_path.clone();
        let mut path = self.path();

        let mut out = ItemPath {
            blob_path: self.blob_path.clone(),
            top_dir: self.into_inner(),
            dirs: vec![],
        };

        for num in get_key_components(key) {
            path = path.join(format!("{num}"));
            if path.exists() {
                let sym = match fs::read_link(&path) {
                    Ok(sym) => sym,
                    Err(e) => {
                        let s = format!("{path:?} {e} {key}");
                        panic!("{}", s);
                    }
                };
                let hash = sym.file_name().unwrap().to_str().unwrap().to_string();
                let dir = Self::load(dir_path.clone(), hash.to_owned())
                    .unwrap()
                    .into_inner();
                out.dirs.push((dir, num));
            } else {
                let dir = Dir::new();
                out.dirs.push((dir, num));
            }
        }
        out
    }
}

fn get_key_components(key: &str) -> Vec<String> {
    let hash = get_hash(&key.to_string());
    let mut out = vec![];
    let mut chars = hash.chars();

    for _ in 0..3 {
        out.push(chars.next().unwrap().to_string());
    }

    out
}

struct ItemPath {
    /// no parent key component on topdir, of course
    top_dir: Dir,
    /// The dirs leading to the item, along with the key component that lead to this dir
    dirs: Vec<(Dir, String)>,
    blob_path: Arc<PathBuf>,
}

impl ItemPath {
    /// Modifies the leaf dir of the itempath, persists all the dirs leading up to it and returns them
    fn modify<F: FnOnce(&mut Dir)>(mut self, dir_modifier: F) -> Vec<FsDir> {
        let mut out = vec![];

        let (item_dir, mut parent_component) = self.dirs.pop().unwrap();

        let mut dir = item_dir;

        dir_modifier(&mut dir);

        let fs_item_dir = dir.persist(self.blob_path.clone());
        let mut path = fs_item_dir.path();
        out.push(fs_item_dir);

        while let Some((dir, cmp)) = self.dirs.pop() {
            let mut dir = dir;
            dir.insert_dir(format!("{parent_component}"), path.clone());
            let fsdir = dir.persist(self.blob_path.clone());
            path = fsdir.path();
            parent_component = cmp;
            out.insert(0, fsdir);
        }

        let mut top_dir = self.top_dir;
        top_dir.insert_dir(format!("{parent_component}"), path);
        let fs_top_dir = top_dir.persist(self.blob_path.clone());
        out.insert(0, fs_top_dir);

        out
    }

    /// Creates a hardlink to an item in the leafdir
    fn save_item(self, key: String, item_path: PathBuf) -> Vec<FsDir> {
        tracing::trace!("inserting item: key: {key}, path: {item_path:?}");

        let f: Box<dyn FnOnce(&mut Dir)> =
            Box::new(|dir: &mut Dir| match dir.insert_file(key, item_path) {
                Some(old) => {
                    tracing::debug!("previous item: {old:?}")
                }
                None => {
                    tracing::trace!("item inserted for first time");
                }
            });

        self.modify(f)
    }

    /// Removes a hardlink to an item in the leafdir
    fn remove_item(self, key: String) -> Vec<FsDir> {
        tracing::trace!("removing item: {key}");
        let f: Box<dyn FnOnce(&mut Dir)> =
            Box::new(|dir: &mut Dir| match dir.contents.remove(&key) {
                Some(old) => {
                    tracing::debug!("item removed: {old:?}");
                }
                None => {
                    tracing::warn!("tried to remove {key}, but it was not present");
                }
            });

        self.modify(f)
    }
}

#[derive(Debug)]
enum Content {
    File(PathBuf),
    Dir(PathBuf),
}

impl Deref for Content {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        match self {
            Content::File(p) => &p,
            Content::Dir(p) => &p,
        }
    }
}

impl Content {
    fn new(path: PathBuf) -> Self {
        if path.is_file() {
            Self::File(path)
        } else {
            Self::Dir(path)
        }
    }

    /// Creates a symlink in case of a directory, hardlink for files.
    /// this is because hardlinks take up less space, but cannot be used for directories
    fn create_file_reference(&self, link: PathBuf) {
        match self {
            Self::File(original) => match fs::hard_link(original, link) {
                Ok(()) => {}
                Err(e) => {
                    dbg!(e);
                }
            },
            Self::Dir(original) => match symlink(original, link) {
                Ok(()) => {}
                Err(e) => {
                    dbg!(e);
                }
            },
        }
    }
}

#[derive(Debug)]
struct Dir {
    contents: HashMap<String, Content>,
}

impl Deref for Dir {
    type Target = HashMap<String, Content>;

    fn deref(&self) -> &Self::Target {
        &self.contents
    }
}

impl Dir {
    fn get_hash(&self) -> Hashed {
        if self.contents.is_empty() {
            get_hash(&())
        } else {
            let mut hash = Hashed::default();
            for (_, val) in self.contents.iter() {
                let entry_hash = val.file_name().unwrap().to_str().unwrap();
                hash.push_str(&entry_hash);
            }

            get_hash(&hash)
        }
    }

    fn new() -> Self {
        Self {
            contents: Default::default(),
        }
    }

    fn insert_dir(&mut self, key: String, path: PathBuf) {
        self.contents.insert(key, Content::Dir(path));
    }

    fn insert_file(&mut self, key: String, path: PathBuf) -> Option<Content> {
        self.contents.insert(key, Content::File(path))
    }

    fn persist(self, dir_path: Arc<PathBuf>) -> FsDir {
        let hash = self.get_hash();
        let path = full_blob_path(&dir_path, &hash);
        fs::create_dir_all(&path).unwrap();

        for (name, original) in self.contents.iter() {
            let link = path.join(name);
            original.create_file_reference(link);
        }

        FsDir::load(dir_path, hash).unwrap()
    }
}

fn full_blob_path(blob_store: &Path, hash: &str) -> PathBuf {
    let mut topdir = String::new();
    let mut chars = hash.chars();
    topdir.push(chars.next().unwrap());
    topdir.push(chars.next().unwrap());
    let dir = blob_store.join(topdir);
    fs::create_dir_all(&dir).ok();
    dir.join(hash)
}
