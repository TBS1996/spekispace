use std::{collections::HashMap, fs, hash::{DefaultHasher, Hash, Hasher}, ops::Deref, os::unix::fs::symlink, path::{Path, PathBuf}, sync::{Arc, Mutex, RwLock}};

use walkdir::WalkDir;

use super::SnapStorage;

type Hashed = String;
type Item = String;
type Key = String;


#[derive(Debug, Clone)]
pub struct SnapFs {
    chain_path: Arc<PathBuf>,
    blob_path: Arc<PathBuf>,
    hashes: Arc<Mutex<Vec<Hashed>>>,
}

impl SnapStorage for SnapFs {
    fn save_on_gen(&self, key: &str, prev_generation: &str, item_hash: &str) -> crate::Hashed {
        let item_path = full_blob_path(&self.blob_path, item_hash);
        let top_map = FsDir::load(self.blob_path.clone(), prev_generation.to_owned()).unwrap();
        let all: ItemPath = top_map.all_dirs(&key);
        let x = all.save_item(key.to_owned(), item_path);
        let top_hash = x.first().unwrap().hash.clone();
        self.save_new_generation(top_hash.clone());
        top_hash
    }

    fn get_all(&self) -> HashMap<String, Vec<u8>> {
        let mut file_map = HashMap::new();

        for entry in WalkDir::new(&*self.blob_path) {
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

    fn get_with_gen(&self, key: &str, gen_hash: &crate::Hashed) -> Option<Vec<u8>> {
        let path = self.full_path(gen_hash, key);
        fs::read(&path).ok()
    }

    fn get_gen(&self, idx: usize) -> crate::Hashed {
        self.hashes.lock().unwrap().get(idx).expect("no such generation").to_owned()
    }

    fn latest_generation(&self) -> crate::Hashed {
        self.hashes.lock().unwrap().last().unwrap().to_owned()
    }
    
    fn save_item(&self, item_hash: &str, item: Vec<u8>) {
        let path = self.the_full_blob_path(item_hash);
        fs::File::create(&path).unwrap();
        fs::write(&path, &item).unwrap();
    }
}


impl SnapFs {
    pub fn new(root: PathBuf) -> Self {
        let chain_path = Arc::new(root.join("chain"));
        let blob_path = Arc::new(root.join("blobs"));
        fs::create_dir_all(&*chain_path).unwrap();
        fs::create_dir_all(&*blob_path).unwrap();
        let mut hash_paths: Vec<PathBuf> = vec![];

        for entry in fs::read_dir(&*chain_path).unwrap() {
            hash_paths.push(entry.unwrap().path());
        }

        hash_paths.sort();
        let hashes: Vec<Hashed> = hash_paths.into_iter().map(|p|fs::read_link(&p).unwrap().file_name().unwrap().to_str().unwrap().to_string()).collect();

        let selv = Self {
            chain_path,
            blob_path,
            hashes: Arc::new(Mutex::new(hashes)),
        };

        selv.save_new_generation(Dir::new(selv.blob_path.clone()).persist().hash);
        selv
    }

    fn the_full_blob_path(&self, hash: &str) -> PathBuf {
        full_blob_path(&self.blob_path, hash)
    }

    fn full_path(&self, _gen: &Hashed, key: &str) -> PathBuf {
        let mut path = self.the_full_blob_path(_gen);
        for cmp in get_key_components(key){
            path = path.join(format!("{cmp}"));
        }

        path = path.join(key);

        path
    }

    fn save_new_generation(&self, hash: Hashed) {
        let name = format!("{:06}", self.hashes.lock().unwrap().len());
        let link = self.chain_path.join(name);
        let original = self.the_full_blob_path(&hash);
        if let Err(e) = symlink(&original, &link) {
            dbg!(original, link, e);
        }
        self.hashes.lock().unwrap().push(hash);
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
        full_blob_path(&self.blob_path, &self.hash)
    }



    /// must only be called from top-level directory
    /// 
    /// returns a vector of all the directories from current to the one where the item of a given key is located.
    /// if the key doesn't exist it'll fill it with empty dir objects.
    fn all_dirs(self, key: &str) -> ItemPath {
        let dir_path = self.blob_path.clone();
        let mut path = self.path();

        let mut out = ItemPath { top_dir: self.into_inner(), dirs: vec![] };

        for num in get_key_components(key){
            path = path.join(format!("{num}"));
            if path.exists() {
                let sym = match fs::read_link(&path) {
                    Ok(sym) => sym,
                    Err(e) => {
                        let s = format!("{path:?} {e} {key}");
                        panic!("{}", s);
                    },
                };
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
}

impl ItemPath {
    fn save_item(mut self, key: String, item_path: PathBuf) -> Vec<FsDir> {
        let mut out = vec![];

        let (item_dir, mut parent_component) = self.dirs.pop().unwrap();

        let mut dir = item_dir;
        dir.insert_file(key, item_path);
        let fs_item_dir = dir.persist();
        let mut path = fs_item_dir.path();
        out.push(fs_item_dir);

        while let Some((dir, cmp)) = self.dirs.pop() {
            let mut dir = dir;
            dir.insert_dir(format!("{parent_component}"), path.clone());
            let fsdir = dir.persist();
            path = fsdir.path();
            parent_component = cmp;
            out.insert(0, fsdir);
        }

        let mut top_dir = self.top_dir;
        top_dir.insert_dir(format!("{parent_component}"), path);
        let fs_top_dir = top_dir.persist();
        out.insert(0, fs_top_dir);

        out
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
    fn new(path: PathBuf) -> Self{
        if path.is_file() {
            Self::File(path)
        } else {
            Self::Dir(path)
        }

    }
}

#[derive(Debug)]
struct Dir{
    blob_path: Arc<PathBuf>,
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

    fn insert_dir(&mut self, key: String, path: PathBuf) {
        self.contents.insert(key, Content::Dir(path));
    }

    fn insert_file(&mut self, key: String, path: PathBuf) {
        self.contents.insert(key, Content::File(path));
    }

    fn persist(self) -> FsDir {
        let dir_path = self.blob_path.clone();
        let hash = self.get_hash();
        let path = full_blob_path(&self.blob_path, &hash);
        fs::create_dir_all(&path).unwrap();

        for (name, original) in self.contents.iter() {
            let link = path.join(name);
            match original {
                Content::File(original) => {
                    match fs::hard_link(original, link) {
                        Ok(_) => {},
                        Err(e) => {
                            dbg!(e);
                        },
                    }
                },
                Content::Dir(original) => {
                    match symlink(original, link) {
                        Ok(_) => {},
                        Err(e) => {
                            dbg!(e);
                        },
                    }

                },
            }
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


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn testlol(){
        let root = PathBuf::from("/home/tor/spekifs/test");
        fs::remove_dir_all(&root).ok();
        let mut snfs = SnapFs::new(root);
        let key = String::from("hey");
        let item = String::from("myitem");
        snfs.save(&key.clone(), item.into_bytes());

        let key = String::from("hey");
        let item = String::from("updateditem");
        snfs.save(&key.clone(), item.into_bytes());

        dbg!(snfs.get_with_gen_idx(&key, 1));
        dbg!(snfs.get_with_gen_idx(&key, 2));

        snfs.save("whatthefuck", "nicethere".into());
        snfs.save("whaffasdf", "foo".into());
        snfs.save("awkjasf", "baz".into());
        snfs.save("whatsfa", "bar".into());
        snfs.save("wosfda", "bar".into());
        snfs.save("wosfsadfa", "bar".into());
        snfs.save("wasdfsa", "bar".into());

    }
}