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
