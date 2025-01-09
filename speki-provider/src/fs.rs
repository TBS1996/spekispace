use std::{
    collections::HashMap,
    fs::{self},
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use async_trait::async_trait;
use rayon::prelude::*;
use speki_dto::{Item, Record, SpekiProvider};
use uuid::Uuid;

fn load_dir_paths<P: AsRef<Path>>(folder_path: P) -> std::io::Result<Vec<PathBuf>> {
    let entries = fs::read_dir(folder_path)?.collect::<Result<Vec<_>, std::io::Error>>()?;

    let paths: Vec<PathBuf> = entries
        .par_iter()
        .filter_map(|entry| {
            let path = entry.path();
            if path.is_file() {
                Some(path)
            } else {
                None
            }
        })
        .collect();

    Ok(paths)
}

pub struct FileProvider {
    base: PathBuf,
}

impl FileProvider {
    pub fn new(base: PathBuf) -> Self {
        Self { base }
    }

    fn item_path(&self, item: &str) -> PathBuf {
        let p = self.base.join(item);
        std::fs::create_dir_all(&p).unwrap();
        p
    }
}

fn load_record_from_path(path: &Path) -> Option<Record> {
    if !path.exists() {
        return None;
    }
    let content = fs::read_to_string(&path).unwrap();
    let id = path.file_name().unwrap().to_str().unwrap().to_string();
    let last_modified = last_modified_path(path).unwrap().as_secs();
    Some(Record {
        id,
        content,
        last_modified,
        inserted: None,
    })
}

fn last_modified_path(path: &Path) -> Option<Duration> {
    Some(
        fs::File::open(path)
            .ok()?
            .metadata()
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap(),
    )
}

#[async_trait(?Send)]
impl<T: Item> SpekiProvider<T> for FileProvider {
    async fn load_record(&self, id: Uuid) -> Option<Record> {
        let p = self.item_path(T::identifier()).join(id.to_string());
        load_record_from_path(&p)
    }

    async fn load_all_records(&self) -> HashMap<Uuid, Record> {
        let mut out = HashMap::default();

        let path = self.item_path(T::identifier());
        for file in load_dir_paths(&path).unwrap() {
            let id: Uuid = file.file_name().unwrap().to_str().unwrap().parse().unwrap();
            let rec = load_record_from_path(&file).unwrap();
            out.insert(id, rec);
        }

        out
    }

    async fn save_record(&self, record: Record) {
        let id = record.id;
        let content = record.content;

        let path = self.item_path(T::identifier()).join(id);
        let mut file = fs::File::create(path).unwrap();
        file.write_all(&mut content.as_bytes()).unwrap();
    }

    async fn current_time(&self) -> Duration {
        use std::time::{SystemTime, UNIX_EPOCH};

        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
    }
}
