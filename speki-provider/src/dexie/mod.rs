use std::time::Duration;

use speki_dto::{CardId, SpekiProvider};
use speki_dto::{Config, Cty};
use uuid::Uuid;

mod js;

pub struct DexieProvider;

use async_trait::async_trait;

#[async_trait(?Send)]
impl SpekiProvider for DexieProvider {
    async fn delete_content(&self, id: Uuid, ty: Cty) {
        js::delete_file(ty, &id.to_string());
    }

    async fn load_all_content(&self, ty: Cty) -> Vec<String> {
        js::load_all_files(ty).await
    }
    async fn save_content(&self, ty: Cty, id: Uuid, content: String) {
        js::save_content(ty, &id.to_string(), &content);
    }

    async fn load_content(&self, id: Uuid, ty: Cty) -> Option<String> {
        js::load_content(ty, &id.to_string()).await
    }

    async fn last_modified(&self, id: Uuid, ty: Cty) -> Option<Duration> {
        js::last_modified(ty, &id.to_string()).await
    }

    async fn load_card_ids(&self) -> Vec<CardId> {
        js::load_ids(Cty::Card)
            .await
            .into_iter()
            .map(|id| CardId(id))
            .collect()
    }

    async fn load_config(&self) -> Config {
        Config
    }

    async fn save_config(&self, _config: Config) {
        todo!()
    }
}
