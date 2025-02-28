use std::{
    any::Any, collections::{BTreeSet, HashMap, HashSet}, fs::{self, read_to_string, File}, io::Write, path::{Path, PathBuf}, time::{Duration, UNIX_EPOCH}
};
use async_trait::async_trait;
use speki_dto::{LedgerEntry, LedgerEvent, ProviderId, RunLedger, LedgerProvider, Storage, TimeProvider};
use uuid::Uuid;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

const STORAGE_DIR: &str = "/home/tor/spekifs";

#[derive(Copy, Clone)]
pub struct FsTime;

impl TimeProvider for FsTime {
    fn current_time(&self) -> Duration {
        Duration::from_secs(std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs())
    }
}


#[derive(Clone)]
pub struct FsProvider {
    time: FsTime,
    id: Option<ProviderId>,
}

impl FsProvider {
    pub fn new() -> Self {
        fs::create_dir_all(STORAGE_DIR).ok();
        Self {
            time: FsTime,
            id: None,
        }
    }

    pub fn set_id(&mut self, id: ProviderId) {
        self.id = Some(id);
    }

    fn file_path(table: &str, id: &str) -> PathBuf {
        Self::table_path(table).join(id)
    }

    fn table_path(table: &str) -> PathBuf {
        let table = Path::new(STORAGE_DIR).join(table);
        std::fs::create_dir_all(&table).unwrap();
        table
    }

    fn write_file(table: &str, id: &str, contents: impl Serialize) {
        let path = Self::file_path(table, id);
        let mut f = File::create(&path).unwrap();
        let contents = serde_json::to_string(&contents).unwrap();
        f.write_all(contents.as_bytes()).unwrap();
    }

    fn load_file<T: DeserializeOwned>(table: &str, id: &str) -> Option<T> {
        let path = Self::file_path(table, id);
        if !path.exists() {
            return None;
        }

        let contents = read_to_string(&path).unwrap();
        let item: T = serde_json::from_str(&contents).unwrap();
        Some(item)
    }
}

pub fn load_file_contents(dir: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(content) = fs::read_to_string(entry.path()) {
                let key = entry.file_name().into_string().unwrap();
                map.insert(key, content);

            }
        }
    }
    map
}


#[async_trait::async_trait(?Send)]
impl<T: Serialize + DeserializeOwned + 'static> Storage<T> for FsProvider {
    async fn load_content(&self, space: &str, id: &str) -> Option<String> {
        let path = Self::file_path(space, id);
        fs::read_to_string(&path).ok()

    }
    async fn load_all_contents(&self, space: &str) -> HashMap<String, String> {
        let dir = Self::table_path(space);
        load_file_contents(&dir)
    }
    async fn save_content(&self, space: &str, id: &str, record: String) {
        Self::write_file(space, &id, record);
    }

    async fn load_ids(&self) -> Vec<String> {
        let ty = <FsProvider as Storage<T>>::item_name(self);
        let dir = Self::table_path(ty);
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


#[async_trait(?Send)]
impl<T: RunLedger<L>, L: LedgerEvent> LedgerProvider<T, L> for FsProvider {
    async fn current_time(&self) -> Duration {
        self.time.current_time()
    }

    async fn save_cache(&self, key: String, ids: HashSet<String>) {
        let space = <FsProvider as LedgerProvider<T, L>>::cache_space(self);
        Self::write_file(&space, &key, ids);
    }

    async fn load_cache(&self, key: &str) -> HashSet<String>{
        let space = <FsProvider as LedgerProvider<T, L>>::cache_space(self);
        Self::load_file(&space, key).unwrap_or_default()
    }

    async fn load_ledger(&self) -> Vec<L>{
        let space = <MemStorage as LedgerProvider<T, L>>::ledger_space(self);

        let map: HashMap<String, String> = <Self as Storage<T>>::load_all_contents(self, &space).await;

        let mut foo: Vec<LedgerEntry<L>> = vec![];

        for (time, value) in map.iter(){
            let time: u64 = time.parse().unwrap();
            let timestamp = Duration::from_micros(time);
            let event: L = serde_json::from_str(value).unwrap();
            let entry = LedgerEntry { timestamp, event };
            foo.push(entry);
        }

        foo.sort_by_key(|k|k.timestamp);
        foo.into_iter().map(|e| e.event).collect()
    }

    /// Clear the storage area so we can re-run everything.
    async fn clear_state(&self) {
        let ty = <FsProvider as Storage<T>>::item_name(self);
        let space = Self::table_path(&ty);
        let path = Path::new(&space);
        if path.exists() {
            fs::remove_dir_all(path).unwrap();
            println!("Deleted directory: {:?}",space);
        } else {
            println!("Directory does not exist: {:?}", space);
        }
    }

    async fn clear_space(&self, _space: &str) {
        panic!()
    }

    async fn clear_ledger(&self) {
        let space = <FsProvider as LedgerProvider<T, L>>::ledger_space(self);
        let path = Path::new(&space);
        if path.exists() {
            fs::remove_dir_all(path).unwrap();
            println!("Deleted directory: {:?}",space);
        } else {
            println!("Directory does not exist: {:?}", space);
        }
    }


    async fn save_ledger(&self, event: LedgerEntry<L>) {
        let space = <FsProvider as LedgerProvider<T, L>>::ledger_space(self);
        let id  = event.timestamp.as_micros().to_string();
        Self::write_file(&space, &id, event.event);
    }
}

/* 
#[async_trait(?Send)]
impl<T: Item> SpekiProvider<T> for FsProvider {
    async fn current_time(&self) -> Duration {
        self.time.current_time()
    }

    async fn load_record(&self, id: T::Key) -> Option<Record> {
        let id = id.to_string();
        let path = Self::file_path(T::identifier(), &id);
        load_record(&path)
    }

    async fn load_all_records(&self) -> HashMap<T::Key, Record> {
        let dir = Self::table_path(T::identifier());
        let mut map = HashMap::new();
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                if let Ok(content) = fs::read_to_string(entry.path()) {
                    if let Ok(record) = serde_json::from_str::<Record>(&content) {
                        if let Ok(key) = serde_json::from_str(&record.id.to_string()) {
                            map.insert(key, record);
                        }
                    }
                }
            }
        }
        map
    }

    async fn save_record(&self, record: Record) {
        use std::io::Write;

        let path = Self::file_path(T::identifier(), &record.id);
        let mut file = File::create(&path).unwrap();
        file.write_all(record.content.as_bytes()).unwrap();
    }
}

use std::io::Write;

#[async_trait(?Send)]
impl<T: Item> Syncable<T> for FsProvider {
    async fn save_id(&self, id: ProviderId) {
        let path = PathBuf::from(STORAGE_DIR).join("provider_id");
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(id.to_string().as_bytes()).unwrap();
    }

    async fn load_id_opt(&self) -> Option<ProviderId> {
        let path = Path::new(STORAGE_DIR).join("provider_id");
        if let Ok(content) = read_to_string(&path){
            serde_json::from_str(&content).ok()
        } else {
            None
        }
    }

    async fn update_sync_info(&self, other: ProviderId, current_time: Duration) {
        let path = Path::new(STORAGE_DIR).join(format!("sync_info_{}", other));
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(current_time.as_secs().to_string().as_bytes())
            .unwrap();
    }

    async fn last_sync(&self, other: ProviderId) -> Duration {
        let path = Path::new(STORAGE_DIR).join(format!("sync_info_{}", other));
        if let Ok(content) = read_to_string(&path){
            if let Ok(seconds) = content.parse::<u64>() {
                return Duration::from_secs(seconds);
            }
        }
        Duration::ZERO
    }
}


#[async_trait(?Send)]
impl<T: Item> Indexable<T> for FsProvider {
    async fn load_indices(&self, word: String) -> BTreeSet<Uuid> {
        todo!()
    }

    async fn save_indices(&self, word: String, indices: BTreeSet<Uuid>) {
        todo!()
    }
}
*/