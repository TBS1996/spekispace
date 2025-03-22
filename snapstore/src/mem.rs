use std::collections::HashMap;
use std::cell::RefCell;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, RwLock};
use super::SnapStorage;

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


#[derive(Clone)]
struct ArcDir(Arc<RwLock<Directory>>);

impl std::fmt::Debug for ArcDir {
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

//struct MapDir(HashMap<u8, ArcDir>);


#[derive(Debug)]
enum DirOrItem{
    Dir(MemDir),
    Item(Arc<String>),
}

impl DirOrItem {
    fn unwrap_as_item(self) -> Arc<String> {
        match self {
            DirOrItem::Dir(_) => panic!(),
            DirOrItem::Item(item) => item.clone(),
        }
    }

    fn unwrap_as_dir(self) -> MemDir {
        match self {
            DirOrItem::Dir(mem_dir) => mem_dir,
            DirOrItem::Item(_) => panic!(),
        }
    }
}


#[derive(Debug, Clone)]
struct MemDir(Arc<RwLock<HashMap<char, DirOrItem>>>);

#[derive(Debug)]
pub struct SnapMem {
    blobs: HashMap<Hashed, DirOrItem>,
    hashes: Vec<Hashed>,
}

impl SnapStorage for SnapMem {
    fn save_on_gen(&mut self, key: &str, prev_generation: &str, item_hash: &str) -> crate::Hashed {
        let item_path = self.save_item(item);
        let top_map = FsDir::load(self.blob_path.clone(), prev_generation.to_owned()).unwrap();
        let all: ItemPath = top_map.all_dirs(&key);
        let x = all.save_item(key.to_owned(), item_path);
        let top_hash = x.first().unwrap().hash.clone();
        self.save_new_generation(top_hash.clone());
        top_hash
    }

    fn get_with_gen(&self, key: &crate::Key, gen_hash: &crate::Hashed) -> Option<crate::Item> {
        todo!()
    }

    fn get_gen(&self, idx: usize) -> crate::Hashed {
        self.hashes.get(idx).unwrap().to_owned()
    }

    fn latest_generation(&self) -> crate::Hashed {
        self.hashes.last().unwrap().to_owned()
    }
    
    fn save_item(&mut self, item_hash: &str, item: String) {
        self.blobs.insert(item_hash.to_owned(), DirOrItem::Item(Arc::new(item)));
    }
}

impl Default for SnapMem {
    fn default() -> Self {
        let mut dirs = HashMap::default();
        let hash = get_hash(&());

        dirs.insert(hash.clone(), Directory::Branch(Default::default()));

        Self { blobs: dirs, hashes: vec![hash] }
    }
}

impl SnapMem {
    pub fn get(&self, key: &str) -> Option<Item>{
        let nums: Vec<u8> = key.as_bytes().iter().cloned().collect();
        let current_hash = self.current_hash();
        let top_dir = self.blobs.get(&current_hash).unwrap();

        top_dir.get(key, nums)
    }

    fn current_hash(&self) -> Hashed {
        self.hashes.last().cloned().unwrap_or_default()
    }

    fn inner_get(&self, key: &str, nums: Vec<u8>) -> Option<Item> {
        None
    }

    pub fn save(&mut self, key: &str, item: String) {
        let current = self.blobs.get(&self.current_hash()).unwrap().clone();
        let new = current.save(key, item);
        let hashed = get_hash(&format!("{:?}",&new));
        self.blobs.insert(hashed.clone(), new);
        self.hashes.push(hashed);
    }
}



mod tests{
    use super::*;

    //#[test]
    fn test_foo(){
        let mut app = SnapMem::default();
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