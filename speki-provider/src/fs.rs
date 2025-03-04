use std::{
    any::Any, collections::{BTreeSet, HashMap, HashSet}, fs::{self, read_to_string, File}, io::Write, path::{Path, PathBuf}, time::{Duration, UNIX_EPOCH}
};
use async_trait::async_trait;
use speki_dto::{LedgerEntry, LedgerEvent, ProviderId, RunLedger, Storage, TimeProvider};
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
    id: Option<ProviderId>,
}


fn write_string(path: &Path, s: String) {
    let mut f = match File::create(&path) {
        Ok(f) => f,
        Err(e) => {
            panic!("error writing to path: {:?} string: {s}, error: {e}", path);
        },
    };
    f.write_all(s.as_bytes()).unwrap();

}

impl FsProvider {
    pub fn new() -> Self {
        fs::create_dir_all(STORAGE_DIR).ok();
        Self {
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
    async fn clear_space(&self, space: &str) {
        let path = Self::table_path(space);
        std::fs::remove_dir_all(&path).unwrap();
    }

    async fn load_content(&self, space: &str, id: &str) -> Option<String> {
        let path = Self::file_path(space, id);
        fs::read_to_string(&path).ok()

    }
    async fn load_all_contents(&self, space: &str) -> HashMap<String, String> {
        let dir = Self::table_path(space);
        load_file_contents(&dir)
    }

    async fn save_content(&self, space: &str, id: &str, record: String) {
        let path = Self::file_path(space, id);
        write_string(&path, record);
    }

    async fn load_ids(&self, space: &str) -> Vec<String> {
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