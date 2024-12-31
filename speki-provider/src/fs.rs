use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;
use std::{
    fs::{self},
    io::Write,
    path::Path,
    time::Duration,
};

use rayon::prelude::*;
use speki_dto::{Cty, ProviderId, ProviderMeta};
use speki_dto::{Record, SpekiProvider};
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

pub struct FileProvider;

use async_trait::async_trait;

fn path_from_ty(ty: Cty) -> PathBuf {
    match ty {
        Cty::Attribute => paths::get_attributes_path(),
        Cty::Review => paths::get_review_path(),
        Cty::Card => paths::get_cards_path(),
    }
}

fn file_path(ty: Cty, id: Uuid) -> PathBuf {
    path_from_ty(ty).join(id.to_string())
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

use speki_dto::Item;

#[async_trait(?Send)]
impl<T: Item> SpekiProvider<T> for FileProvider {
    async fn load_record(&self, id: Uuid, ty: Cty) -> Option<Record> {
        let p = file_path(ty, id);
        load_record_from_path(&p)
    }

    async fn provider_id(&self) -> ProviderId {
        todo!()
    }

    async fn update_sync(&self, other: ProviderId, ty: Cty, current_time: Duration) {
        todo!()
    }

    async fn last_sync(&self, other: ProviderId, ty: Cty) -> Duration {
        todo!()
    }

    async fn load_all_records(&self, ty: Cty) -> HashMap<Uuid, Record> {
        let mut out = HashMap::default();

        let path = path_from_ty(ty);
        for file in load_dir_paths(&path).unwrap() {
            let id: Uuid = file.file_name().unwrap().to_str().unwrap().parse().unwrap();
            let rec = load_record_from_path(&file).unwrap();
            out.insert(id, rec);
        }

        out
    }

    async fn save_record(&self, ty: Cty, record: Record) {
        let id = record.id;
        let content = record.content;

        let path = file_path(ty, id.parse().unwrap());
        let mut file = fs::File::create(path).unwrap();
        file.write_all(&mut content.as_bytes()).unwrap();
    }
}

pub mod paths {

    #![allow(dead_code)]

    use std::{
        fs::{self, create_dir_all},
        path::PathBuf,
    };

    pub fn get_cache_path() -> PathBuf {
        let path = dirs::home_dir().unwrap().join(".cache").join("speki");
        create_dir_all(&path).unwrap();
        path
    }

    pub fn config_dir() -> PathBuf {
        let path = dirs::home_dir().unwrap().join(".config").join("speki");
        fs::create_dir_all(&path).unwrap();
        path
    }

    pub fn get_review_path() -> PathBuf {
        let path = get_share_path().join("reviews");
        create_dir_all(&path).unwrap();
        path
    }

    pub fn get_collections_path() -> PathBuf {
        let path = get_share_path().join("collections");
        create_dir_all(&path).unwrap();
        path
    }

    pub fn get_concepts_path() -> PathBuf {
        let path = get_share_path().join("concepts");
        create_dir_all(&path).unwrap();
        path
    }

    pub fn get_attributes_path() -> PathBuf {
        let path = get_share_path().join("attributes");
        create_dir_all(&path).unwrap();
        path
    }

    pub fn get_cards_path() -> PathBuf {
        let path = get_share_path().join("cards");
        create_dir_all(&path).unwrap();
        path
    }

    #[cfg(not(test))]
    pub fn get_share_path() -> PathBuf {
        let home = dirs::home_dir().unwrap();
        home.join(".local/share/speki/")
    }

    #[cfg(test)]
    pub fn get_share_path() -> PathBuf {
        PathBuf::from("./test_dir/")
    }
}
