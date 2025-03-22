use serde::{de::DeserializeOwned, Serialize};
use speki_dto::{ItemStorage, LedgerEvent, LedgerStorage, ProviderId, Storage, TimeProvider};
use std::{
    cell::Ref, collections::{HashMap, HashSet}, fmt::{Debug, Display}, fs::{self, hard_link, File}, io::Write, ops::{Deref, DerefMut}, path::{Path, PathBuf}, sync::{Arc, RwLock}, time::Duration
};

const STORAGE_DIR: &str = "/home/tor/spekifs";

#[derive(Copy, Clone)]
pub struct FsTime;

impl TimeProvider for FsTime {
    fn current_time(&self) -> Duration {
        Duration::from_secs(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        )
    }
}

#[derive(Clone)]
pub struct FsProvider {
    id: Option<ProviderId>,
}

fn write_bytes(path: &Path, s: &[u8]) {
    let mut f = match File::create(&path) {
        Ok(f) => f,
        Err(e) => {
            panic!("error writing to path: {:?} bytes: {s:?}, error: {e}", path);
        }
    };
    f.write_all(s).unwrap();
}

impl FsProvider {
    pub fn new() -> Self {
        fs::create_dir_all(STORAGE_DIR).ok();
        Self { id: None }
    }

    pub fn set_id(&mut self, id: ProviderId) {
        self.id = Some(id);
    }

    fn file_path(table: &[&str], id: &str) -> PathBuf {
        Self::table_path(table).join(id)
    }

    fn table_path(namespace: &[&str]) -> PathBuf {
        let mut table = PathBuf::from(STORAGE_DIR);

        for x in namespace {
            table = table.join(x);
        }

        std::fs::create_dir_all(&table).unwrap();
        table
    }
}

pub fn load_file_contents(dir: &Path) -> HashMap<String, Vec<u8>> {
    let mut map = HashMap::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(content) = fs::read(entry.path()) {
                let key = entry.file_name().into_string().unwrap();
                map.insert(key, content);
            }
        }
    }
    map
}

#[async_trait::async_trait(?Send)]
impl<T: Serialize + DeserializeOwned + std::fmt::Debug + 'static, E: LedgerEvent>
    LedgerStorage<T, E> for FsProvider
{
    async fn load_refdeps(&self, _id: &str, _deptype: &str) -> HashSet<String> {
        unimplemented!()
    }

    async fn load_property_cache(&self, property: &str, value: &str) -> HashSet<String> {
        let mut ns = <FsProvider as LedgerStorage<T, E>>::property_cache_ns(self, property);
        ns.push(value);
        let path = Self::file_path(&ns, property);
        load_file_contents(&path).into_keys().collect()
    }

    async fn save_property_cache(&self, property: &str, value: &str, ids: HashSet<String>) {
        for id in ids {
            <FsProvider as LedgerStorage<T, E>>::insert_property_cache(self, property, value, id)
                .await;
        }
    }

    async fn insert_property_cache(&self, property: &str, value: &str, id: String) {
        let mut ns = <FsProvider as LedgerStorage<T, E>>::property_cache_ns(self, property);
        ns.push(value);
        let from = Self::file_path(&ns, &id);

        let to = {
            let x = <FsProvider as LedgerStorage<T, E>>::blob_ns(self);
            Self::file_path(&x, &id)
        };

        if let Err(e) = hard_link(to, from) {
            dbg!(e);
        }
    }

    async fn remove_from_property_cache(&self, property: &str, value: &str, id: &str) {
        let mut ns = <FsProvider as LedgerStorage<T, E>>::property_cache_ns(self, property);
        ns.push(value);
        let path = Self::file_path(&ns, &id);
        std::fs::remove_file(&path).unwrap();
    }

    async fn save_refdep(&self, id: &str, dep_type: &str, reff: &str) {
        let mut x = <FsProvider as LedgerStorage<T, E>>::ref_cache_ns(self);
        x.push(id);
        x.push(dep_type);

        let from = Self::file_path(&x, reff);

        let to = {
            let x = <FsProvider as LedgerStorage<T, E>>::blob_ns(self);
            Self::file_path(&x, reff)
        };

        if let Err(e) = hard_link(to, from) {
            dbg!(e);
        }
    }

    async fn load_refdep_items(&self, id: &str, deptype: &str) -> HashMap<String, T> {
        let mut x = <FsProvider as LedgerStorage<T, E>>::ref_cache_ns(self);
        x.push(id);
        x.push(deptype);

        let path = Self::file_path(&x, id);
        load_file_contents(&path)
            .into_iter()
            .map(|(key, val)| (key, serde_json::from_slice(&val).unwrap()))
            .collect()
    }
}

#[async_trait::async_trait(?Send)]
impl Storage for FsProvider {
    async fn clear_space(&self, space: &[&str]) {
        let path = Self::table_path(space);
        std::fs::remove_dir_all(&path).unwrap();
    }

    async fn load_content(&self, space: &[&str], id: &str) -> Option<Vec<u8>> {
        let path = Self::file_path(space, id);
        fs::read(path).ok()
    }

    async fn load_all_contents(&self, space: &[&str]) -> HashMap<String, Vec<u8>> {
        let dir = Self::table_path(space);
        load_file_contents(&dir)
    }

    async fn save_content(&self, space: &[&str], id: &str, content: &[u8]) {
        let path = Self::file_path(space, id);
        write_bytes(&path, content);
    }

    async fn load_ids(&self, space: &[&str]) -> Vec<String> {
        let dir = Self::table_path(space);
        let mut map = vec![];
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let key = entry.file_name().into_string().unwrap();
                map.push(key);
            }
        }
        map
    }
}

#[async_trait::async_trait(?Send)]
impl<T: Serialize + DeserializeOwned + std::fmt::Debug + Sized + 'static> ItemStorage<T>
    for FsProvider
{
    async fn xload_item(&self, space: &[&str], id: &str) -> Option<T> {
        let bytes = self.load_content(space, id).await?;
        //let x: T = bincode::deserialize(&bytes).unwrap();
        let x: T = serde_json::from_slice(&bytes).unwrap();
        Some(x)
    }

    async fn xload_all_items(&self, space: &[&str]) -> HashMap<String, T> {
        dbg!(&space);
        let contents = self.load_all_contents(space).await;
        let mut out: HashMap<String, T> = Default::default();

        for (key, val) in contents {
            let val: T = serde_json::from_slice(&val).unwrap();
            out.insert(key, val);
        }

        out
    }

    async fn xsave_item(&self, space: &[&str], id: &str, item: &T) {
        dbg!(item);
        //let bytes = bincode::serialize(item).unwrap();
        let bytes = serde_json::to_vec(item).unwrap();
        self.save_content(space, id, &bytes).await;
    }
}

pub mod fs_snap {
    use std::{fs::{read_dir, read_link, read_to_string}, hash::{DefaultHasher, Hash, Hasher}, os::unix::fs::symlink};

    use super::*;

    type Hashed = String;
    type Item = String;
    type Key = String;


    fn get_hash<T: Hash>(item: &T) -> Hashed {
        let mut hasher = DefaultHasher::new();
        item.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    #[derive(Debug)]
    struct Dir{
        blob_path: Arc<PathBuf>,
        contents: HashMap<String, PathBuf>,
    }

    impl Deref for Dir {
        type Target = HashMap<String, PathBuf>;
    
        fn deref(&self) -> &Self::Target {
            &self.contents
        }
    }

    /// A Dir as it exists on the filesystem, cannot be mutated inmemory, only acquired by loading from fs.
    #[derive(Debug)]
    struct FsDir {
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
            let path = dir_path.join(&hash);
            if !path.exists() {
                return None;
            }

            let mut contents: HashMap<String, PathBuf> = Default::default();

            for entry in std::fs::read_dir(path).unwrap() {
                let entry = entry.unwrap();
                let name = entry.file_name().into_string().unwrap();
                let path = entry.path();
                dbg!(&path);
                let sympath = std::fs::read_link(&path).unwrap();
                contents.insert(name, sympath);
            }

            let dir = Dir {
                blob_path: dir_path,
                contents,
            };

            Some(Self {
                hash,
                dir,
            })
        }

        fn into_inner(self) -> Dir {
            self.dir
        }

        fn path(&self) -> PathBuf {
            self.blob_path.join(&self.hash)
        }



        /// must only be called from top-level directory
        /// 
        /// returns a vector of all the directories from current to the one where the item of a given key is located.
        /// if the key doesn't exist it'll fill it with empty dir objects.
        fn all_dirs(self, key: &Key) -> ItemPath {
            let dir_path = self.blob_path.clone();
            let components: Vec<u8> = key.as_bytes().iter().cloned().collect();
            let mut path = self.path();

            let mut out = ItemPath { top_dir: self.into_inner(), dirs: vec![] };

            for num in components {
                path = path.join(format!("{num}"));
                if path.exists() {
                    let sym = read_link(&path).unwrap(); 
                    let hash = sym.file_name().unwrap().to_str().unwrap().to_string();
                    let dir = Self::load(dir_path.clone(), hash.to_owned()).unwrap().into_inner();
                    out.dirs.push((dir, num));
                } else {
                    let dir = Dir::new(dir_path.clone());
                    out.dirs.push((dir, num));
                }
            }

            out
        }
    }

    struct ItemPath {
        /// no parent key component on topdir, of course
        top_dir: Dir,
        /// The dirs leading to the item, along with the key component that lead to this dir
        dirs: Vec<(Dir, u8)>,
    }

    impl ItemPath {
        fn save_item(mut self, key: String, item_path: PathBuf) -> Vec<FsDir> {
            let mut out = vec![];
            dbg!();

            let (item_dir, mut parent_component) = self.dirs.pop().unwrap();

            let mut dir = item_dir;
            dir.insert(key, item_path);
            let fs_item_dir = dir.persist();
            let mut path = fs_item_dir.path();
            out.push(fs_item_dir);

            while let Some((dir, cmp)) = self.dirs.pop() {
                dbg!(cmp);
                let mut dir = dir;
                dir.insert(format!("{parent_component}"), path.clone());
                let fsdir = dir.persist();
                path = fsdir.path();
                parent_component = cmp;
                out.insert(0, fsdir);
            }

            let mut top_dir = self.top_dir;
            top_dir.insert(format!("{parent_component}"), path);
            dbg!();
            let fs_top_dir = top_dir.persist();
            out.insert(0, fs_top_dir);

            out
        }
    }



    impl Dir {
        fn get_hash(&self) -> Hashed {
            let mut hash = get_hash(&());

            for (_, val) in self.contents.iter() {
                let entry_hash = val.file_name().unwrap().to_str().unwrap();
                hash.push_str(&entry_hash);
            }

            get_hash(&hash)
        }

        fn new(dir_path: Arc<PathBuf>) -> Self {
            Self {
                blob_path: dir_path,
                contents: Default::default(),
            }
        }

        fn insert(&mut self, key: String, path: PathBuf) {
            self.contents.insert(key, path);
        }

        fn persist(self) -> FsDir {
            dbg!("persisting dir:", &self);
            let dir_path = self.blob_path.clone();
            let hash = self.get_hash();
            let path = dir_path.join(&hash);
            dbg!(&path);
            std::fs::create_dir_all(&path).unwrap();

            for (name, original) in self.contents.iter() {
                let link = path.join(name);
                dbg!(&link, &original);
                symlink(original, link).unwrap();
            }

            FsDir::load(dir_path, hash).unwrap()
        }
    }


    #[derive(Debug)]
    pub struct SnapFs {
        chain_path: Arc<PathBuf>,
        blob_path: Arc<PathBuf>,
        hashes: Vec<Hashed>,
    }

    impl  SnapFs {
        pub fn new(root: PathBuf) -> Self {
            let chain_path = Arc::new(root.join("chain"));
            let blob_path = Arc::new(root.join("blobs"));
            fs::create_dir_all(&*chain_path).unwrap();
            fs::create_dir_all(&*blob_path).unwrap();
            let mut hash_paths: Vec<PathBuf> = vec![];

            for entry in read_dir(&*chain_path).unwrap() {
                hash_paths.push(entry.unwrap().path());
            }

            hash_paths.sort();
            let hashes: Vec<Hashed> = hash_paths.into_iter().map(|p|read_link(&p).unwrap().file_name().unwrap().to_str().unwrap().to_string()).collect();


            let mut selv = Self {
                chain_path,
                blob_path,
                hashes,
            };

            selv.save_new_generation(Dir::new(selv.blob_path.clone()).persist().hash);
            selv
        }

        pub fn save_item(&self, item: Item) -> PathBuf {
            let item_hash = get_hash(&item);
            let path = self.blob_path.join(item_hash);
            std::fs::File::create(&path).unwrap();
            std::fs::write(&path, &item).unwrap();
            path

        }

        fn components(key: &Key) -> Vec<u8> {
            key.as_bytes().iter().cloned().collect()
        }

        fn full_path(&self, gen: &Hashed, key: &Key) -> PathBuf {
            let mut path = self.blob_path.join(gen);
            for cmp in Self::components(key) {
                path = path.join(format!("{cmp}"));
            }

            path = path.join(key);

            path
        }

        pub fn load_item(&self, key: &Key) -> Option<Item> {
            let hash = self.current_hash();
            let path = self.full_path(&hash, key);
            read_to_string(&path).ok()
        }

        fn save_new_generation(&mut self, hash: Hashed) {
            let name = format!("{:06}", self.hashes.len());
            let link = self.chain_path.join(name);
            let original = self.blob_path.join(&hash);
            dbg!(&link, &original);
            symlink(original, link).unwrap();
            self.hashes.push(hash);
        }

        /// 1. save item in blob store with its hash as key
        /// 2. get current full path of all FsDir to the item's current location in a vector
        /// 3. take the last fsdir in the vector and clone it and add the new item there
        /// 4. it'll have a new hash because of the upserted item, so save the dir as a new fsdir in the dir folder
        /// 5. do the same for all the other dirs leading to the item
        /// 6. the top-level item will then represent the new generation and the hash of that will be the new curent hash
        fn save(&mut self, key: Key, item: Item) {
            dbg!("saving item");
            let item_path = self.save_item(item);
            let current_gen = self.current_hash();
            let top_map = FsDir::load(self.blob_path.clone(), current_gen).unwrap();
            dbg!("getting item path");
            let all: ItemPath = top_map.all_dirs(&key);
            dbg!("saving new item path");
            let x = all.save_item(key, item_path);
            let top_hash = x.first().unwrap().hash.clone();
            dbg!("saving new generation");
            self.save_new_generation(top_hash);
        }

        fn current_hash(&self) -> Hashed {
            self.hashes.last().cloned().unwrap()
        }
    }

    mod tests {
        use std::fs::remove_dir_all;

        use super::*;

        #[test]
        fn testlol(){
            let root = PathBuf::from("/home/tor/spekifs/test");
            remove_dir_all(&root).ok();
            let mut snfs = SnapFs::new(root);
            let key = String::from("hey");
            let item = String::from("myitem");
            snfs.save(key.clone(), item);
            dbg!(snfs.load_item(&key));
        }
    }
}


pub mod mem_snap {
    use super::*;
type Hashed = String;
type Item = String;
type Key = String;

/// For each iteration down the chain you get either a list of hashes that lead to other maps
/// or the bottom of it where the actual items are
#[derive(Debug, Clone)]
enum Directory {
    Branch(MapDir),
    Leaf(ItemDir),
}

#[derive(Debug, Clone, Default)]
struct ItemDir(HashMap<Key, Arc<RwLock<Item>>>);

impl ItemDir {
    fn hash(&self) -> Hashed {
        todo!()
    }
}

use std::cell::RefCell;

#[derive(Clone)]
struct ArcDir(Arc<RwLock<Directory>>);

impl Debug for ArcDir {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = format!("{:?}", *self.0.read().unwrap());

        write!(f, "{}", s)
    }
}

impl Deref for ArcDir {
    type Target = Arc<RwLock<Directory>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ArcDir {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}


impl Deref for ArcItem {
    type Target = Arc<RwLock<Item>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

struct ArcItem(Arc<RwLock<Item>>);

#[derive(Debug, Clone, Default)]
struct MapDir(HashMap<u8, ArcDir>);


use std::hash::{DefaultHasher, Hash, Hasher};
fn get_hash<T: Hash>(item: &T) -> Hashed {
    let mut hasher = DefaultHasher::new();
    item.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

impl Directory {
    fn get(&self, key: &str, mut nums: Vec<u8>) -> Option<Item> {
        let mut current: ArcDir = match self {
            Directory::Branch(map_dir) => map_dir.0.get(&nums.pop().unwrap()).unwrap().clone(),
            Directory::Leaf(item_dir) => todo!(),
        };

        while let Some(num) = nums.pop() {
            let x = match &*current.read().unwrap() {
                Directory::Leaf(_) => panic!(),
                Directory::Branch(map) => {
                    map.0.get(&num).unwrap().clone()
                },
            };

            current = x;
        }

        let x = current.read().unwrap();

        match &*x{
            Directory::Leaf(items) => items.0.get(key).map(|x|x.read().unwrap().clone()),
            Directory::Branch(_) => panic!(),
        }
    }

    fn save(self, key: &str, item: Item) -> Self {
        let mut nums: Vec<u8> = key.as_bytes().iter().cloned().collect();

        let mut list: Vec<(Option<u8>, Self)> = vec![(None, self)];

        while let Some(num) = nums.pop() {
            let prev = match &list.last().unwrap().1 {
                Directory::Leaf(_) => unreachable!(),
                Directory::Branch(map) => {
                    map.0.get(&num).map(|x|x.read().unwrap().clone()).unwrap_or_else(||{
                        if nums.is_empty() {
                            Self::Leaf(Default::default())
                        } else {
                            Self::Branch(Default::default())
                        }
                    })
                },
            };

            list.push((Some(num), prev));
        }

        let (mut prev_num, mut prev_map) = list.pop().unwrap();

        match &mut prev_map {
            Directory::Branch(_) => unreachable!(),
            Directory::Leaf(item_dir) => {
                item_dir.0.insert(key.to_string(), Arc::new(RwLock::new(item)));
            },
        }

        while let Some((num, mut map)) = list.pop() {
            match &mut map {
                Directory::Branch(map_dir) => {
                    map_dir.0.insert(prev_num.unwrap(), ArcDir(Arc::new(RwLock::new(prev_map.clone()))));
                },
                Directory::Leaf(_) => unreachable!(),
            }

            prev_num = num;
            prev_map = map;
        }

        prev_map
    }
}

#[derive(Debug)]
pub struct SnapshotStorage {
    dirs: HashMap<Hashed, Directory>,
    hashes: Vec<Hashed>,
}

impl Default for SnapshotStorage {
    fn default() -> Self {
        let mut dirs = HashMap::default();
        let hash = get_hash(&());

        dirs.insert(hash.clone(), Directory::Branch(Default::default()));

        Self { dirs, hashes: vec![hash] }
    }
}

impl SnapshotStorage {
    pub fn get(&self, key: &str) -> Option<Item>{
        let nums: Vec<u8> = key.as_bytes().iter().cloned().collect();
        let current_hash = self.current_hash();
        let top_dir = self.dirs.get(&current_hash).unwrap();

        top_dir.get(key, nums)
    }

    fn current_hash(&self) -> Hashed {
        self.hashes.last().cloned().unwrap_or_default()
    }

    fn inner_get(&self, key: &str, nums: Vec<u8>) -> Option<Item> {
        None
    }

    pub fn save(&mut self, key: &str, item: String) {
        let current = self.dirs.get(&self.current_hash()).unwrap().clone();
        let new = current.save(key, item);
        let hashed = get_hash(&format!("{:?}",&new));
        self.dirs.insert(hashed.clone(), new);
        self.hashes.push(hashed);
    }
}

}


mod tests{
    use super::*;
    use super::mem_snap::*;

    //#[test]
    fn test_foo(){
        let mut app = SnapshotStorage::default();
        let key = "mykey";
        let item = "hey world".to_string();
        app.save(key, item);
        let key = "yourkey";
        let item = "hey there".to_string();
        app.save(key, item);
        dbg!(&app);
        dbg!(app.get("mykey"));
        panic!();
    }
}