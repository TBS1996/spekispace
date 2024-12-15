use std::time::Duration;
use std::{collections::HashMap, path::PathBuf};

use async_trait::async_trait;
use speki_dto::{CardId, Config, Cty, Record, SpekiProvider};
use uuid::Uuid;

mod js;

pub struct BrowserFsProvider {
    repo: PathBuf,
}

impl BrowserFsProvider {
    pub fn new(path: &str) -> Self {
        Self {
            repo: PathBuf::from(path),
        }
    }

    fn review_path(&self) -> PathBuf {
        self.repo.join("reviews")
    }

    fn attrs_path(&self) -> PathBuf {
        self.repo.join("attributes")
    }

    fn cards_path(&self) -> PathBuf {
        self.repo.join("cards")
    }

    fn folder_path(&self, ty: Cty) -> PathBuf {
        match ty {
            Cty::Attribute => self.attrs_path(),
            Cty::Review => self.review_path(),
            Cty::Card => self.cards_path(),
        }
    }

    fn content_path(&self, ty: Cty, id: Uuid) -> PathBuf {
        self.folder_path(ty).join(id.to_string())
    }
}

#[async_trait(?Send)]
impl SpekiProvider for BrowserFsProvider {
    async fn load_record(&self, id: Uuid, ty: Cty) -> Option<Record> {
        todo!()
    }

    async fn load_all_records(&self, ty: Cty) -> HashMap<Uuid, Record> {
        todo!()
    }

    async fn delete_content(&self, id: Uuid, ty: Cty) {
        js::delete_file(self.content_path(ty, id));
    }

    async fn load_all_content(&self, ty: Cty) -> Vec<String> {
        js::load_all_files(self.folder_path(ty)).await
    }

    async fn save_content(&self, ty: Cty, id: Uuid, content: String) {
        js::save_file(self.content_path(ty, id), &content);
    }

    async fn load_content(&self, id: Uuid, ty: Cty) -> Option<String> {
        js::load_file(self.content_path(ty, id)).await
    }

    async fn last_modified(&self, id: Uuid, ty: Cty) -> Option<Duration> {
        js::last_modified(self.content_path(ty, id)).await
    }

    async fn load_card_ids(&self) -> Vec<CardId> {
        js::load_filenames(self.folder_path(Cty::Card).to_str().unwrap())
            .await
            .into_iter()
            .map(|id| CardId(id.parse().unwrap()))
            .collect()
    }

    async fn load_config(&self) -> Config {
        Config
    }

    async fn save_config(&self, _config: Config) {
        todo!()
    }
}
