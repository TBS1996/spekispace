use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    fs::{self, hard_link},
    hash::Hash,
    io::{self, ErrorKind},
    marker::PhantomData,
    ops::Deref,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    sync::Arc,
};

use tracing::trace;
use walkdir::WalkDir;

use crate::{CacheKey, HashAndContents, KeyFoo, get_hash};
use rayon::prelude::*;

type Hashed = String;

/*

todo: add tests showing that rebuild and modify will end up with same hash


*/

//pub enum CacheKey<> {

#[derive(Clone)]
pub struct CacheFs<PK: Display + Clone = String, RK: Display + Clone = String> {
    pub blob_path: Arc<PathBuf>,
    pub key_components: usize,
    _phantom: PhantomData<(PK, RK)>,
}

impl<PK: Display + Clone, RK: Display + Clone> CacheFs<PK, RK> {
    pub fn new(root: PathBuf, key_components: usize) -> Self {
        let blob_path = Arc::new(root.join("blobs"));
        fs::create_dir_all(&*blob_path).unwrap();

        Self {
            blob_path,
            key_components,
            _phantom: PhantomData,
        }
    }

    fn get_key_components(&self, key: &str) -> Vec<char> {
        let hash = get_hash(&key.to_string());
        let mut out = vec![];
        let mut chars = hash.chars();

        for _ in 0..self.key_components {
            out.push(chars.next().unwrap());
        }

        tracing::trace!("key: {key}, components: {out:?}");

        out
    }

    pub fn the_full_blob_path(&self, hash: &str) -> PathBuf {
        full_blob_path(&self.blob_path, hash)
    }

    pub fn full_path_leaf(&self, _gen: &str, key: &str) -> PathBuf {
        trace!("get full leaf path of key: {key} on gen : {_gen}");
        let mut path = self.the_full_blob_path(_gen);
        for cmp in self.get_key_components(key) {
            path = path.join(format!("{cmp}"));
        }

        path
    }

    pub fn full_path(&self, _gen: &str, key: &str) -> PathBuf {
        trace!("get full path of key: {key} on gen : {_gen}");
        let mut path = self.full_path_leaf(_gen, key);
        path = path.join(key);

        path
    }

    pub fn save_cache(
        &self,
        gen_hash: Option<&str>,
        key: &CacheKey<PK, RK>,
        mut items: Vec<String>,
        new_content: &mut Vec<Content>,
    ) -> HashAndContents {
        items.sort();

        let mut prev_items: HashSet<String> = match gen_hash {
            Some(hash) => self.get_cache(hash, key).into_iter().collect(),
            None => Default::default(),
        };

        prev_items.extend(items);

        let mut items: Vec<String> = prev_items.into_iter().collect();
        items.sort();

        let key = key.to_string();

        let key = KeyFoo {
            key: &key,
            cmps: self.get_key_components(&key),
        };

        let item_hash = get_hash(&items);
        let items: String = items.join("\n");
        self.save_item(&item_hash, items.as_bytes().to_vec());
        self.save_on_gen(key, gen_hash, &item_hash, new_content)
    }

    fn get_leafdir_path(
        &self,
        prev_generation: Option<&str>,
        cmps: Vec<char>,
        new_content: &mut Vec<Content>,
    ) -> ItemPath {
        let top_map = match prev_generation {
            Some(prev_gen) => FsDir::load(self.blob_path.clone(), prev_gen.to_owned()).unwrap(),
            None => {
                let empty = Dir::new();
                empty.persist(self.blob_path.clone(), new_content).0
            }
        };

        top_map.all_dirs(cmps)
    }

    pub fn save_on_gen(
        &self,
        key: KeyFoo<'_>,
        prev_generation: Option<&str>,
        item_hash: &str,
        new_content: &mut Vec<Content>,
    ) -> HashAndContents {
        let item_path = full_blob_path(&self.blob_path, item_hash);

        let leafdir_path = self.get_leafdir_path(prev_generation, key.cmps, new_content);
        let x = leafdir_path.save_item(key.key.to_owned(), item_path, new_content);

        let top_hash = x.first().unwrap().0.hash.clone();
        trace!("new generation after item insert: {top_hash}");
        let mut new_contents: Vec<Content> = Default::default();

        for (dir, already_existed) in x {
            if !already_existed {
                let c = Content::new(dir.path());
                new_contents.push(c);
            }
        }

        (top_hash, new_contents)
    }

    pub fn save_item(&self, item_hash: &str, item: Vec<u8>) {
        let path = self.the_full_blob_path(item_hash);
        fs::File::create(&path).unwrap();
        fs::write(&path, &item).unwrap();
    }

    pub fn remove_cache(
        &self,
        gen_hash: &str,
        cache_key: &CacheKey<PK, RK>,
        item: &str,
        new_content: &mut Vec<Content>,
    ) -> HashAndContents {
        let mut prev_items: HashSet<String> =
            self.get_cache(gen_hash, cache_key).into_iter().collect();

        assert!(prev_items.remove(item));

        let mut items: Vec<String> = prev_items.into_iter().collect();
        items.sort();

        let key = cache_key.to_string();

        let key = KeyFoo {
            key: &key,
            cmps: self.get_key_components(&key),
        };

        let item_hash = get_hash(&items);
        let items: String = items.join("\n");
        self.save_item(&item_hash, items.as_bytes().to_vec());
        self.save_on_gen(key, Some(gen_hash), &item_hash, new_content)
    }

    pub fn get_cache(&self, gen_hash: &str, cache_key: &CacheKey<PK, RK>) -> Vec<String> {
        let path = self.full_path(gen_hash, &cache_key.to_string());
        let mut out = vec![];

        if !path.exists() {
            return vec![];
        }

        for line in fs::read_to_string(&path).unwrap().lines() {
            let line: String = line.parse().unwrap();
            out.push(line);
        }

        out
    }
}

#[derive(Debug, Clone)]
pub struct SnapFs {
    pub blob_path: Arc<PathBuf>,
}

impl SnapFs {
    pub fn all_paths(&self, gen_hashh: &str) -> HashSet<Content> {
        let base_path = self.the_full_blob_path(gen_hashh);

        // Read the top-level entries (assuming these are all dirs)
        let dirs: Vec<_> = fs::read_dir(&base_path)
            .unwrap()
            .map(|e| e.unwrap()) // unwrap each entry explicitly
            .filter(|e| e.path().is_dir())
            .map(|e| e.path())
            .collect();

        // Process each top-level dir in parallel
        dirs.par_iter()
            .flat_map_iter(|dir| {
                WalkDir::new(dir)
                    .follow_links(true)
                    .into_iter()
                    .map(|entry| entry.unwrap()) // crash on any walkdir error
                    .map(|entry| Content::new(entry.path().to_path_buf()))
            })
            .collect()
    }
    pub fn xall_paths(&self, gen_hashh: &str) -> HashSet<Content> {
        let base_path = self.the_full_blob_path(gen_hashh);

        // Read the top-level entries (assuming these are all dirs)
        let dirs: Vec<_> = fs::read_dir(&base_path)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.path().is_dir())
            .map(|e| e.path())
            .collect();

        // Process each top-level dir in parallel
        dirs.par_iter()
            .flat_map_iter(|dir| {
                WalkDir::new(dir)
                    .follow_links(true)
                    .into_iter()
                    .filter_map(Result::ok)
                    .map(|entry| Content::new(entry.path().to_path_buf()))
            })
            .collect()
    }

    pub fn get_all(&self, hash: &str) -> HashMap<String, Vec<u8>> {
        let path = dbg!(self.the_full_blob_path(hash));
        let mut file_map = HashMap::new();

        for entry in WalkDir::new(&path).follow_links(true) {
            let Ok(entry) = entry else {
                dbg!(entry);
                continue;
                //panic!();
            };
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

    fn get_leafdir_path(
        &self,
        prev_generation: Option<&str>,
        cmps: Vec<char>,
        new_content: &mut Vec<Content>,
    ) -> ItemPath {
        let top_map = match prev_generation {
            Some(prev_gen) => FsDir::load(self.blob_path.clone(), prev_gen.to_owned()).unwrap(),
            None => {
                let empty = Dir::new();
                empty.persist(self.blob_path.clone(), new_content).0
            }
        };

        top_map.all_dirs(cmps)
    }

    pub fn save_on_gen(
        &self,
        key: KeyFoo<'_>,
        prev_generation: Option<&str>,
        item_hash: &str,
        new_content: &mut Vec<Content>,
    ) -> HashAndContents {
        let item_path = full_blob_path(&self.blob_path, item_hash);
        if !item_path.exists() {
            dbg!(key, prev_generation, item_hash);
            panic!();
        }

        let leafdir_path = self.get_leafdir_path(prev_generation, key.cmps, new_content);
        let x = leafdir_path.save_item(key.key.to_owned(), item_path, new_content);

        let top_hash = x.first().unwrap().0.hash.clone();
        trace!("new generation after item insert: {top_hash}");
        let mut new_contents: Vec<Content> = Default::default();

        for (dir, already_existed) in x {
            if !already_existed {
                let c = Content::new(dir.path());
                new_contents.push(c);
            }
        }

        (top_hash, new_contents)
    }

    pub fn get(&self, hash: &str, key: &str) -> Option<Vec<u8>> {
        trace!("try get item: {key} on hash: {hash}");
        let path = self.full_path(hash, key);
        fs::read(&path).ok()
    }

    pub fn delete_on_gen(
        &self,
        key: KeyFoo<'_>,
        prev_generation: Option<&str>,
        new_content: &mut Vec<Content>,
    ) -> HashAndContents {
        let leafdir_path = self.get_leafdir_path(prev_generation, key.cmps, new_content);
        let x = leafdir_path.remove_item(key.key.to_owned(), new_content);

        let top_hash = x.first().unwrap().0.hash.clone();
        trace!("new generation after item removal: {top_hash}");
        let mut new_contents: Vec<Content> = Default::default();

        for (dir, already_existed) in x {
            if !already_existed {
                let c = Content::new(dir.path());
                new_contents.push(c);
            }
        }

        (top_hash, new_contents)
    }

    pub fn save_item(&self, item_hash: &str, item: Vec<u8>) -> (bool, PathBuf) {
        let path = self.the_full_blob_path(item_hash);
        let new = path.is_file();
        fs::File::create(&path).unwrap();
        fs::write(&path, &item).unwrap();
        (new, path)
    }

    pub fn remove(
        &self,
        gen_hash: &str,
        key: &str,
        new_content: &mut Vec<Content>,
    ) -> HashAndContents {
        let topdir = FsDir::load(self.blob_path.clone(), gen_hash.to_string()).unwrap();
        let keycomps = self.get_key_components(key);
        let all = topdir.all_dirs(keycomps);
        let new_path = all.remove_item(key.to_owned(), new_content);
        let hash = new_path.first().unwrap().0.hash.clone();

        let mut contents: Vec<Content> = vec![];

        for (dir, already_existed) in new_path {
            if !already_existed {
                let c = Content::new(dir.path());
                contents.push(c);
            }
        }

        (hash, contents)
    }

    /// Saves the item and returns the hash to the new generation and list of added paths.
    pub fn save(
        &self,
        gen_hash: Option<&str>,
        key: &str,
        item: Vec<u8>,
        new_content: &mut Vec<Content>,
    ) -> HashAndContents {
        let key = KeyFoo {
            key,
            cmps: self.get_key_components(key),
        };
        let item_hash = get_hash(&item);
        if let (true, path) = self.save_item(&item_hash, item) {
            new_content.push(Content::new(path));
        }

        self.save_on_gen(key, gen_hash, &item_hash, new_content)
    }

    pub fn num_key_components(&self) -> usize {
        3
    }

    pub fn get_key_components(&self, key: &str) -> Vec<char> {
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

impl SnapFs {
    pub fn new(root: PathBuf) -> Self {
        let blob_path = Arc::new(root.join("blobs"));
        fs::create_dir_all(&*blob_path).unwrap();

        Self { blob_path }
    }

    pub fn the_full_blob_path(&self, hash: &str) -> PathBuf {
        full_blob_path(&self.blob_path, hash)
    }

    fn full_path_leaf(&self, _gen: &str, key: &str) -> PathBuf {
        trace!("get full leaf path of key: {key} on gen : {_gen}");
        let mut path = self.the_full_blob_path(_gen);
        for cmp in self.get_key_components(key) {
            path = path.join(format!("{cmp}"));
        }

        path
    }

    pub fn full_path(&self, _gen: &str, key: &str) -> PathBuf {
        trace!("get full path of key: {key} on gen : {_gen}");
        let mut path = self.full_path_leaf(_gen, key);
        path = path.join(key);

        path
    }
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
        trace!("loading dir: {dir_path:?}");
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
    fn all_dirs(self, key_comps: Vec<char>) -> ItemPath {
        trace!("retrieving full itempath of key: {key_comps:?}");
        let dir_path = self.blob_path.clone();
        let mut path = self.path();

        let mut out = ItemPath {
            blob_path: self.blob_path.clone(),
            top_dir: self.into_inner(),
            dirs: vec![],
        };

        for num in key_comps {
            path = path.join(format!("{num}"));
            if path.exists() {
                let sym = match fs::read_link(&path) {
                    Ok(sym) => sym,
                    Err(e) => {
                        let s = format!("{path:?} {e} ");
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

/// Represents the full path to a given leafdir.
#[derive(Debug)]
struct ItemPath {
    /// no parent key component on topdir, of course
    top_dir: Dir,
    /// The dirs leading to the item, along with the key component that lead to this dir
    dirs: Vec<(Dir, char)>,
    blob_path: Arc<PathBuf>,
}

impl ItemPath {
    /// Modifies the leaf dir of the itempath, persists all the dirs leading up to it and returns them
    fn modify<F: FnOnce(&mut Dir)>(
        mut self,
        dir_modifier: F,
        new_content: &mut Vec<Content>,
    ) -> Vec<(FsDir, bool)> {
        let mut out = vec![];

        let (item_dir, mut parent_component) = self.dirs.pop().unwrap();

        let mut dir = item_dir;

        dir_modifier(&mut dir);

        let fs_item_dir = dir.persist(self.blob_path.clone(), new_content);
        let mut path = fs_item_dir.0.path();
        out.push(fs_item_dir);

        while let Some((dir, cmp)) = self.dirs.pop() {
            let mut dir = dir;
            dir.insert_dir(format!("{parent_component}"), path.clone());
            let fsdir = dir.persist(self.blob_path.clone(), new_content);
            path = fsdir.0.path();
            parent_component = cmp;
            out.insert(0, fsdir);
        }

        let mut top_dir = self.top_dir;
        top_dir.insert_dir(format!("{parent_component}"), path);
        let fs_top_dir = top_dir.persist(self.blob_path.clone(), new_content);
        out.insert(0, fs_top_dir);

        out
    }

    /// Creates a hardlink to an item in the leafdir
    fn save_item(
        self,
        key: String,
        item_path: PathBuf,
        new_content: &mut Vec<Content>,
    ) -> Vec<(FsDir, bool)> {
        tracing::trace!("inserting item: key: {key}, path: {item_path:?}");
        assert!(item_path.exists());

        let f: Box<dyn FnOnce(&mut Dir)> =
            Box::new(|dir: &mut Dir| match dir.insert_file(key, item_path) {
                Some(old) => {
                    tracing::debug!("previous item: {old:?}")
                }
                None => {
                    tracing::trace!("item inserted for first time");
                }
            });

        self.modify(f, new_content)
    }

    /// Removes a hardlink to an item in the leafdir
    fn remove_item(self, key: String, new_content: &mut Vec<Content>) -> Vec<(FsDir, bool)> {
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

        self.modify(f, new_content)
    }
}

#[derive(Debug, Hash, Eq, PartialEq)]
pub enum Content {
    File(PathBuf),
    Dir(PathBuf),
    Symlink(PathBuf),
}

impl Deref for Content {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        match self {
            Content::File(p) => p,
            Content::Dir(p) => p,
            Content::Symlink(p) => p,
        }
    }
}

impl Content {
    pub fn new(path: PathBuf) -> Self {
        let path = match path.canonicalize() {
            Ok(p) => p,
            Err(e) => {
                dbg!(path);
                dbg!(e);
                panic!();
            }
        };

        if path.is_file() {
            Self::File(path)
        } else if path.is_symlink() {
            Self::Symlink(path)
        } else if path.is_dir() {
            Self::Dir(path)
        } else {
            dbg!(path);
            panic!()
        }
    }

    pub fn delete(self) -> io::Result<()> {
        match self {
            Content::File(p) => fs::remove_file(&p),
            Content::Symlink(p) => {
                std::fs::remove_file(p).unwrap();
                Ok(())
            }
            Content::Dir(p) => {
                std::fs::remove_dir_all(&p).unwrap();
                Ok(())
            }
        }
    }

    /// Creates a symlink in case of a directory, hardlink for files.
    /// this is because hardlinks take up less space, but cannot be used for directories
    fn create_file_reference(&self, link: PathBuf) {
        match self {
            Self::File(original) => match hard_link(original, &link) {
                Ok(()) => {}
                Err(e) => {}
            },
            Self::Symlink(original) => {
                dbg!(original, link);
                panic!("idk man");
            }
            Self::Dir(original) => {
                if !original.exists() {
                    dbg!(original);
                    panic!();
                }

                match symlink(original, link) {
                    Ok(()) => {}
                    Err(e) => {
                        if e.kind() != ErrorKind::AlreadyExists {
                            dbg!(e);
                            panic!();
                        }
                    }
                }
            }
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
                hash.push_str(entry_hash);
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
        assert!(path.is_dir());
        self.contents.insert(key, Content::Dir(path));
    }

    fn insert_file(&mut self, key: String, path: PathBuf) -> Option<Content> {
        assert!(path.is_file());
        self.contents.insert(key, Content::File(path))
    }

    fn persist(self, dir_path: Arc<PathBuf>, new_content: &mut Vec<Content>) -> (FsDir, bool) {
        let hash = self.get_hash();

        let path = full_blob_path(&dir_path, &hash);
        if !path.exists() {
            fs::create_dir_all(&path).unwrap();
            new_content.push(Content::new(path.clone()))
        }

        for (name, original) in self.contents.iter() {
            let link = path.join(name);
            if !link.exists() {
                original.create_file_reference(link.clone());
                assert!(link.exists());
                new_content.push(Content::new(link));
            }
        }

        (FsDir::load(dir_path, hash).unwrap(), false)
    }
}

fn full_blob_path(blob_store: &Path, hash: &str) -> PathBuf {
    let mut topdir = String::new();
    let mut chars = hash.chars();
    for _ in 0..2 {
        topdir.push(chars.next().unwrap());
    }
    let dir = blob_store.join(topdir);
    fs::create_dir_all(&dir).ok();
    dir.join(hash)
}
