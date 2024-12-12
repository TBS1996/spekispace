use std::collections::{HashMap, HashSet};
use std::{fmt::Debug, sync::Arc, time::Duration};

use speki_core::{AnyType, Attribute, Card, TimeProvider};
use speki_dto::{AttributeId, CardId};
use speki_provider::DexieProvider;
use speki_provider::IndexBaseProvider;
use tracing::info;

use crate::firebase::sign_in;
use crate::firebase::FirestoreProvider;
use crate::{js, login::LoginState, PROXY, REPO_PATH};

pub mod cookies {
    use std::collections::HashMap;
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(inline_js = "
export function getCookies() {
    const cookies = document.cookie;
    console.log('Cookies:', cookies);
    return cookies;
}
")]
    extern "C" {
        fn getCookies() -> String;
    }

    pub fn get(key: &str) -> Option<String> {
        parse_cookies(&getCookies()).get(key).cloned()
    }

    fn parse_cookies(cookie_header: &str) -> HashMap<String, String> {
        cookie_header
            .split("; ")
            .filter_map(|cookie| {
                let parts: Vec<&str> = cookie.split('=').collect();
                if parts.len() == 2 {
                    Some((parts[0].to_string(), parts[1].to_string()))
                } else {
                    None
                }
            })
            .collect()
    }
}

pub fn get_install_token() -> Option<String> {
    cookies::get("install-token")
}

pub fn get_auth_token() -> Option<String> {
    cookies::get("auth-token")
}

pub fn sync_repo(info: LoginState) {
    if let Some(token) = info.auth_token() {
        js::_sync_repo(REPO_PATH, &token, PROXY);
    }
}

#[derive(Clone)]
pub struct App(pub Arc<speki_core::App>);

impl AsRef<speki_core::App> for App {
    fn as_ref(&self) -> &speki_core::App {
        &self.0
    }
}

impl Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("App").field(&self.0).finish()
    }
}

struct WasmTime;

impl TimeProvider for WasmTime {
    fn current_time(&self) -> Duration {
        js::current_time()
    }
}

impl App {
    pub fn new() -> Self {
        Self(Arc::new(speki_core::App::new(
            DexieProvider,
            speki_core::SimpleRecall,
            WasmTime,
        )))
    }
}

async fn sync_two(app1: App, app2: App) {
    let mut map1: HashMap<CardId, Card<AnyType>> = app1
        .0
        .load_all_cards()
        .await
        .into_iter()
        .map(|card| (card.id, Arc::unwrap_or_clone(card)))
        .collect();

    let mut map2: HashMap<CardId, Card<AnyType>> = app2
        .0
        .load_all_cards()
        .await
        .into_iter()
        .map(|card| (card.id, Arc::unwrap_or_clone(card)))
        .collect();

    let mut ids: HashSet<CardId> = map1.keys().map(|key| *key).collect();
    ids.extend(map2.keys());

    ids.extend(app2.0.card_provider.load_all_card_ids().await);

    for id in ids {
        info!("syncing id...");

        match (map1.remove(&id), map2.remove(&id)) {
            (None, None) => panic!(),
            (None, Some(card)) => app1.0.save_card(card).await,
            (Some(card), None) => app2.0.save_card(card).await,
            (Some(card1), Some(card2)) => {
                if card1.reviews().len() != card2.reviews().len() {
                    info!("merging reviews");
                    let mut revs = card1.reviews().clone();
                    revs.extend(card2.reviews().clone());

                    app1.0
                        .card_provider
                        .save_reviews(card1.id, speki_core::reviews::Reviews(revs.clone()))
                        .await;

                    app2.0
                        .card_provider
                        .save_reviews(card1.id, speki_core::reviews::Reviews(revs))
                        .await;
                }

                if card1.last_modified > card2.last_modified {
                    info!("saving card 1");
                    app2.0.save_card_not_reviews(card1).await;
                } else if card1.last_modified < card2.last_modified {
                    info!("saving card 2");
                    app1.0.save_card_not_reviews(card2).await;
                }
            }
        }
    }

    let mut map1: HashMap<AttributeId, Attribute> = app1
        .0
        .load_all_attributes()
        .await
        .into_iter()
        .map(|attr| (attr.id, attr))
        .collect();

    let mut map2: HashMap<AttributeId, Attribute> = app2
        .0
        .load_all_attributes()
        .await
        .into_iter()
        .map(|attr| (attr.id, attr))
        .collect();

    let mut ids: HashSet<AttributeId> = map1.keys().map(|key| *key).collect();
    ids.extend(map2.keys());

    for id in ids {
        info!("syncing attr id");
        match (map1.remove(&id), map2.remove(&id)) {
            (None, None) => panic!(),
            (None, Some(attr)) => app1.0.save_attribute(attr).await,
            (Some(attr), None) => app2.0.save_attribute(attr).await,
            (Some(attr1), Some(attr2)) => {
                if attr1.last_modified > attr2.last_modified {
                    info!("saving attr 1");
                    app2.0.save_attribute(attr1).await;
                } else if attr1.last_modified < attr2.last_modified {
                    info!("saving attr 2");
                    app1.0.save_attribute(attr2).await;
                }
            }
        }
    }
}

pub async fn sync() {
    let agent = sign_in().await;
    let firestore_app = App(Arc::new(speki_core::App::new(
        FirestoreProvider::new(agent),
        speki_core::SimpleRecall,
        WasmTime,
    )));

    let dexie_app = App(Arc::new(speki_core::App::new(
        DexieProvider,
        speki_core::SimpleRecall,
        WasmTime,
    )));

    info!("lets sync!!!!!!!!!!!!!!!!!!!!!");

    sync_two(firestore_app, dexie_app).await;

    info!("done syncing maybe!");
}
