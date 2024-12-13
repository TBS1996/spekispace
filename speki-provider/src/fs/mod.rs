use rayon::prelude::*;
use speki_dto::SpekiProvider;
use speki_dto::{Config, Cty};
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

fn load_files<P: AsRef<Path>>(folder_path: P) -> std::io::Result<Vec<String>> {
    let entries = fs::read_dir(folder_path)?.collect::<Result<Vec<_>, std::io::Error>>()?;

    let contents: Vec<String> = entries
        .par_iter()
        .filter_map(|entry| {
            let path = entry.path();
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

#[async_trait(?Send)]
impl SpekiProvider for FileProvider {
    async fn load_content(&self, id: Uuid, ty: Cty) -> Option<String> {
        let path = file_path(ty, id);

        if !path.exists() {
            None
        } else {
            Some(fs::read_to_string(&path).unwrap())
        }
    }

    async fn last_modified(&self, id: Uuid, ty: Cty) -> Option<Duration> {
        Some(
            fs::File::open(file_path(ty, id))
                .ok()?
                .metadata()
                .unwrap()
                .modified()
                .unwrap()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap(),
        )
    }
    async fn load_all_content(&self, ty: Cty) -> Vec<String> {
        load_files(path_from_ty(ty)).unwrap()
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
