use rayon::prelude::*;
use speki_dto::{Config, Cty};
use speki_dto::{Record, SpekiProvider};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;
use std::{
    fs::{self},
    io::{Read, Write},
    path::Path,
    time::Duration,
};
use uuid::Uuid;

pub mod paths;

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

fn _load_files<P: AsRef<Path>>(folder_path: P) -> std::io::Result<Vec<String>> {
    let contents: Vec<String> = load_dir_paths(folder_path)?
        .par_iter()
        .filter_map(|path| {
            if path.is_file() {
                let mut file_content = String::new();
                if let Ok(mut file) = fs::File::open(&path) {
                    if file.read_to_string(&mut file_content).is_ok() {
                        return Some(file_content);
                    }
                }
            }
            None
        })
        .collect();

    Ok(contents)
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
    let last_modified = last_modified_path(path).unwrap().as_secs();
    Some(Record {
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

#[async_trait(?Send)]
impl SpekiProvider for FileProvider {
    async fn load_record(&self, id: Uuid, ty: Cty) -> Option<Record> {
        let p = file_path(ty, id);
        load_record_from_path(&p)
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

    async fn save_content(&self, ty: Cty, id: Uuid, content: String) {
        let path = file_path(ty, id);
        let mut file = fs::File::create(path).unwrap();
        file.write_all(&mut content.as_bytes()).unwrap();
    }

    async fn delete_content(&self, id: Uuid, ty: Cty) {
        let path = file_path(ty, id);
        fs::remove_file(&path).unwrap();
    }

    async fn load_config(&self) -> Config {
        Config
    }

    async fn save_config(&self, _config: Config) {
        todo!()
    }
}
