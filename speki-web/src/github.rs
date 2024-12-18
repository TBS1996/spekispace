use dioxus::prelude::*;
use tracing::{debug, info};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use crate::js;

pub const REPO_PATH: &'static str = "/foobar";
pub const PROXY: &'static str = "http://127.0.0.1:8081";

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct UserInfo {
    pub auth_token: String,
    pub install_token: String,
    pub username: Signal<String>,
}

#[derive(Clone, Default)]
pub struct LoginState {
    pub inner: Signal<Option<UserInfo>>,
}

impl LoginState {
    pub fn auth_token(&self) -> Option<String> {
        self.inner.cloned()?.auth_token.into()
    }

    pub async fn load_cached(&mut self) -> bool {
        info!("loading credentials from cache");
        if let Some(info) = load_cached_info().await {
            info!("successfully loaded credentials from cache");
            self.inner.set(Some(info));
            true
        } else {
            info!("couldn't load credentials from cache");
            false
        }
    }

    pub async fn load_uncached(&mut self) -> bool {
        if let Some(info) = load_user_info().await {
            self.inner.set(Some(info));
            true
        } else {
            false
        }
    }
}

pub async fn load_cached_info() -> Option<UserInfo> {
    let auth_token = get_auth_token()?;
    let res = fetch_github_username(auth_token.clone()).await;
    debug!("{:?}", &res);
    let username = res.ok()?;
    let install_token = get_install_token().unwrap();
    Some(UserInfo {
        auth_token,
        username: Signal::new(username),
        install_token,
    })
}

async fn load_user_info() -> Option<UserInfo> {
    info!("connecting to auth server...");
    let auth_url = "http://localhost:3000/auth/github";
    web_sys::window()
        .unwrap()
        .location()
        .set_href(auth_url)
        .unwrap();
    let auth_token = get_auth_token()?;
    let res = fetch_github_username(auth_token.clone()).await;
    info!("{:?}", &res);
    let username = res.ok()?;
    let install_token = get_install_token().unwrap();
    Some(UserInfo {
        auth_token,
        username: Signal::new(username),
        install_token,
    })
}

#[wasm_bindgen]
pub async fn fetch_github_username(access_token: String) -> Result<String, JsValue> {
    use serde::Deserialize;
    use serde_wasm_bindgen::from_value;
    use web_sys::{Request, RequestInit, RequestMode, Response};

    #[derive(Deserialize)]
    struct GithubUser {
        login: String,
    }

    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::Cors);

    // GitHub API endpoint for the user data
    let url = "https://api.github.com/user";
    let request = Request::new_with_str_and_init(url, &opts)?;

    // Set Authorization header with the access token
    request
        .headers()
        .set("Authorization", &format!("token {}", access_token))?;

    // Make the request
    let window = web_sys::window().expect("no global `window` exists");
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into().unwrap();

    // Check if the response is successful
    if resp.ok() {
        let json = JsFuture::from(resp.json()?).await?;
        let user: GithubUser = from_value(json)
            .map_err(|e| JsValue::from_str(&format!("Failed to parse user data: {:?}", e)))?;
        Ok(user.login)
    } else {
        Err(JsValue::from_str("Failed to fetch GitHub user data"))
    }
}

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
