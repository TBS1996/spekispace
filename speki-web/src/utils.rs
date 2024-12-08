use std::{fmt::Debug, sync::Arc, time::Duration};

use speki_core::TimeProvider;
use speki_provider::DexieProvider;
use speki_provider::IndexBaseProvider;
use tracing::info;

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
        js::sync_repo(REPO_PATH, &token, PROXY);
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

pub async fn _sync() {
    let dexie_app = App(Arc::new(speki_core::App::new(
        DexieProvider,
        speki_core::SimpleRecall,
        WasmTime,
    )));

    let fs_app = App(Arc::new(speki_core::App::new(
        IndexBaseProvider::new("/foobar"),
        speki_core::SimpleRecall,
        WasmTime,
    )));

    info!("loading fs cards");
    let cards = fs_app.0.load_all_cards().await;

    for card in cards {
        info!("saving card to dexie");
        dexie_app.0.save_card(Arc::unwrap_or_clone(card)).await;
    }

    info!("loading fs attrs");
    let attrs = fs_app.0.load_all_attributes().await;
    for attr in attrs {
        dexie_app.0.save_attribute(attr).await;
    }

    info!("done syncing maybe!");
}
