#![allow(non_snake_case)]

use crate::provider::IndexBaseProvider;
use dioxus::prelude::*;
use dioxus_logger::tracing::{info, Level};
use serde::Deserialize;
use speki_core::{AnyType, App, Card, TimeProvider};
use speki_dto::{CardId, Review, SpekiProvider};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use wasm_bindgen::prelude::*;
use web_sys::console;

mod pages;
mod provider;

mod cookies {
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

use wasm_bindgen_futures::JsFuture;

fn get_install_token() -> Option<String> {
    cookies::get("install-token")
}
fn get_auth_token() -> Option<String> {
    cookies::get("auth-token")
}

pub fn log_to_console<T: std::fmt::Debug>(val: T) -> T {
    let message = format!("{:?}", &val);
    console::log_1(&JsValue::from_str(&message));
    val
}

use crate::pages::{Add, Debug, Home, Review, View};

#[derive(Clone, Routable, Debug, PartialEq)]
enum Route {
    #[route("/")]
    Home {},
    #[route("/review")]
    Review {},
    #[route("/view/:id")]
    View { id: String },
    #[route("/add")]
    Add {},
    #[route("/debug")]
    Debug {},
}

pub mod js {
    use std::time::Duration;

    use gloo_utils::format::JsValueSerdeExt;
    use js_sys::Promise;
    use serde_json::Value;
    use wasm_bindgen::prelude::*;

    use crate::log_to_console;

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = Date)]
        fn now() -> f64;
    }

    #[wasm_bindgen(module = "/assets/utils.js")]
    extern "C" {
        fn cloneRepo(path: &JsValue, url: &JsValue, token: &JsValue, proxy: &JsValue);
        fn gitClone(path: &JsValue, url: &JsValue, token: &JsValue, proxy: &JsValue);
        fn fetchRepo(path: &JsValue, url: &JsValue, token: &JsValue, proxy: &JsValue);
        fn pullRepo(path: &JsValue, token: &JsValue, proxy: &JsValue);
        fn syncRepo(path: &JsValue, token: &JsValue, proxy: &JsValue);
        fn loadAllFiles(path: &JsValue) -> Promise;
        fn loadRec(path: &JsValue) -> Promise;
        fn newReviews(path: &JsValue) -> Promise;
        fn listFiles(path: &JsValue);
        fn allPaths(path: &JsValue) -> Promise;
        fn deleteFile(path: &JsValue);
        fn loadFile(path: &JsValue) -> Promise;
        fn saveFile(path: &JsValue, content: &JsValue);
        fn validateUpstream(path: &JsValue, token: &JsValue);
        fn deleteDir(path: &JsValue);
    }

    pub fn current_time() -> Duration {
        Duration::from_millis(now() as u64)
    }

    pub fn delete_dir(path: &str) {
        let path = JsValue::from_str(path);
        deleteDir(&path);
    }

    pub fn fetch_repo(path: &str, url: &str, token: &str, proxy: &str) {
        let path = JsValue::from_str(path);
        let url = JsValue::from_str(url);
        let token = JsValue::from_str(token);
        let proxy = JsValue::from_str(proxy);
        fetchRepo(&path, &url, &token, &proxy);
    }

    pub fn clone_repo(path: &str, url: &str, token: &str, proxy: &str) {
        let path = JsValue::from_str(path);
        let url = JsValue::from_str(url);
        let token = JsValue::from_str(token);
        let proxy = JsValue::from_str(proxy);
        cloneRepo(&path, &url, &token, &proxy);
        //gitClone(&path, &url, &token, &proxy);
    }

    pub fn validate_upstream(path: &str, token: &str) {
        let path = JsValue::from_str(path);
        let token = JsValue::from_str(token);
        validateUpstream(&path, &token);
    }

    pub fn sync_repo(path: &str, token: &str, proxy: &str) {
        log_to_console("lets sync :D");
        let path = JsValue::from_str(path);
        let token = JsValue::from_str(token);
        let proxy = JsValue::from_str(proxy);
        syncRepo(&path, &token, &proxy);
    }

    pub fn pull_repo(path: &str, token: &str, proxy: &str) {
        log_to_console("starting pull repo");
        let path = JsValue::from_str(path);
        let token = JsValue::from_str(token);
        let proxy = JsValue::from_str(proxy);
        pullRepo(&path, &token, &proxy);
        log_to_console("rs pull repo ended");
    }

    pub fn delete_file(path: &str) {
        let path = JsValue::from_str(path);
        deleteFile(&path);
    }

    pub async fn list_files(path: &str) -> Value {
        let path = JsValue::from_str(path);
        let val = promise_to_val(allPaths(&path)).await;
        val
    }

    pub fn save_file(path: &str, content: &str) {
        let path = JsValue::from_str(path);
        let content = JsValue::from_str(content);
        saveFile(&path, &content);
    }

    async fn promise_to_val(promise: Promise) -> Value {
        let future = wasm_bindgen_futures::JsFuture::from(promise);
        let jsvalue = future.await.unwrap();
        jsvalue.into_serde().unwrap()
    }

    pub async fn git_status(path: &str) -> u64 {
        let path = JsValue::from_str(path);
        let val = promise_to_val(newReviews(&path)).await;
        crate::log_to_console(&val);
        match val {
            serde_json::Value::Number(s) => s.as_u64().unwrap(),
            _ => panic!("damn"),
        }
    }

    pub async fn load_all_files_rec(path: &str) -> Vec<String> {
        let path = JsValue::from_str(path);
        let val = promise_to_val(loadRec(&path)).await;
        let arr = val.as_array().unwrap();
        arr.into_iter()
            .map(|elm| match elm {
                Value::String(s) => s.clone(),
                other => panic!("file isnt textfile damn: {}", other),
            })
            .collect()
    }

    pub async fn load_all_files(path: &str) -> Vec<String> {
        let path = JsValue::from_str(path);
        let val = promise_to_val(loadAllFiles(&path)).await;
        let arr = val.as_array().unwrap();
        arr.into_iter()
            .map(|elm| match elm {
                Value::String(s) => s.clone(),
                other => panic!("file isnt textfile damn: {}", other),
            })
            .collect()
    }

    pub async fn load_file(path: &str) -> Option<String> {
        let path = JsValue::from_str(path);
        let val = promise_to_val(loadFile(&path)).await;

        match val {
            Value::Null => None,
            Value::String(s) => Some(s.clone()),
            other => panic!("invalid type: {}", other),
        }
    }
}

struct WasmTime;

impl TimeProvider for WasmTime {
    fn current_time(&self) -> Duration {
        js::current_time()
    }
}

fn main() {
    dioxus_logger::init(Level::INFO).expect("failed to init logger");
    info!("starting app");
    launch(App);
}

fn App() -> Element {
    use_context_provider(State::new);
    use_context_provider(ReviewState::default);
    rsx! {
        Router::<Route> {}
    }
}

#[wasm_bindgen]
pub async fn fetch_github_username(access_token: String) -> Result<String, JsValue> {
    // Import necessary items within the function for encapsulation
    use serde::Deserialize;
    use serde_wasm_bindgen::from_value;
    use web_sys::{Request, RequestInit, RequestMode, Response};

    #[derive(Deserialize)]
    struct GithubUser {
        login: String,
    }

    // Initialize the request
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

pub async fn load_cached_info() -> Option<UserInfo> {
    let auth_token = get_auth_token()?;
    let res = fetch_github_username(auth_token.clone()).await;
    log_to_console(&res);
    let username = res.ok()?;
    let install_token = get_install_token().unwrap();
    Some(UserInfo {
        auth_token,
        username,
        install_token,
    })
}

async fn load_user_info() -> Option<UserInfo> {
    let auth_url = "http://localhost:3000/auth/github";
    web_sys::window()
        .unwrap()
        .location()
        .set_href(auth_url)
        .unwrap();
    let auth_token = get_auth_token()?;
    let res = fetch_github_username(auth_token.clone()).await;
    log_to_console(&res);
    let username = res.ok()?;
    let install_token = get_install_token().unwrap();
    Some(UserInfo {
        auth_token,
        username,
        install_token,
    })
}

#[derive(Clone)]
pub struct State {
    inner: Arc<Mutex<InnerState>>,
    app: Arc<App>,
}

impl State {
    pub fn new() -> Self {
        let app = App::new(
            IndexBaseProvider::new("/foobar"),
            speki_core::SimpleRecall,
            WasmTime,
        );
        Self {
            inner: Default::default(),
            app: Arc::new(app),
        }
    }

    pub fn info(&self) -> Signal<Option<UserInfo>> {
        self.inner.lock().unwrap().token.clone()
    }
}

#[derive(Debug)]
pub struct UserInfo {
    auth_token: String,
    install_token: String,
    username: String,
}

#[derive(Default)]
struct InnerState {
    token: Signal<Option<UserInfo>>,
}

#[derive(Default, Clone)]
struct ReviewState {
    card: Signal<Option<Card<AnyType>>>,
    queue: Arc<Mutex<Vec<CardId>>>,
    tot_len: Signal<usize>,
    pos: Signal<usize>,
    front: Signal<String>,
    back: Signal<String>,
}

impl ReviewState {
    fn id(&self) -> Option<CardId> {
        Some(self.card.as_ref()?.id())
    }

    async fn refresh(&self, app: &App, filter: String) {
        let cards = app.load_non_pending(Some(filter)).await;
        self.tot_len.clone().set(cards.len());
        {
            let mut lock = self.queue.lock().unwrap();
            *lock = cards;
        }
        self.next_card(app, "/foobar").await;
    }

    async fn make_review(&self, review: Review, repo: &str) {
        if let Some(id) = self.id() {
            IndexBaseProvider::new(repo).add_review(id, review).await;
        }
    }

    fn current_pos(&self) -> usize {
        self.tot_len - self.queue.lock().unwrap().len()
    }

    async fn do_review(&self, app: &App, review: Review, repo: &str) {
        self.make_review(review, repo).await;
        self.next_card(app, repo).await;
    }

    async fn next_card(&self, app: &App, repo: &str) {
        let card = self.queue.lock().unwrap().pop();

        let card = match card {
            Some(id) => {
                let card = Card::from_raw(
                    app.foobar.clone(),
                    IndexBaseProvider::new(repo).load_card(id).await.unwrap(),
                )
                .await;
                let front = card.print().await;
                let back = card
                    .display_backside()
                    .await
                    .unwrap_or_else(|| "___".to_string());

                self.front.clone().set(front);
                self.back.clone().set(back);
                Some(card)
            }
            None => None,
        };

        self.card.clone().set(card);
        self.pos.clone().set(self.current_pos());
    }
}
