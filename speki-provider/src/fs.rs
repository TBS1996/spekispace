use serde::{de::DeserializeOwned, Serialize};
use speki_dto::{ItemStorage, LedgerEvent, LedgerStorage, ProviderId, Storage, TimeProvider};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, hard_link, File},
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
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
        let x: T = bincode::deserialize(&bytes).unwrap();
        Some(x)
    }

    async fn xload_all_items(&self, space: &[&str]) -> HashMap<String, T> {
        dbg!(&space);
        let contents = self.load_all_contents(space).await;
        let mut out: HashMap<String, T> = Default::default();

        for (key, val) in contents {
            let val: T = bincode::deserialize(&val).unwrap();
            out.insert(key, val);
        }

        out
    }

    async fn xsave_item(&self, space: &[&str], id: &str, item: &T) {
        dbg!(item);
        let bytes = bincode::serialize(item).unwrap();
        self.save_content(space, id, &bytes).await;
    }
}
