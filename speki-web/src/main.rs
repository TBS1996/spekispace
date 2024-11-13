#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_logger::tracing::{info, Level};
use js::load_all_files;
use serde::Deserialize;
use speki_dto::SpekiProvider;
use std::sync::{Arc, Mutex};
use wasm_bindgen::prelude::*;
use web_sys::console;

mod provider;
mod utils;

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

#[derive(Clone, Routable, Debug, PartialEq)]
enum Route {
    #[route("/")]
    Home {},
}

pub mod js {
    use futures::executor::block_on;
    use gloo_utils::format::JsValueSerdeExt;
    use js_sys::Promise;
    use serde_json::Value;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::future_to_promise;

    use crate::log_to_console;

    #[wasm_bindgen(module = "/assets/utils.js")]
    extern "C" {
        fn loadAllFiles(path: &JsValue) -> Promise;
        fn cloneRepo(path: &JsValue, url: &JsValue, token: &JsValue);
        fn listFiles(path: &JsValue);
        fn deleteFile(path: &JsValue);
        fn loadFile(path: &JsValue) -> Promise;
        fn saveFile(path: &JsValue, content: &JsValue);
    }

    pub fn clone_repo(path: &str, url: &str, token: &str) {
        let path = JsValue::from_str(path);
        let url = JsValue::from_str(url);
        let token = JsValue::from_str(token);
        cloneRepo(&path, &url, &token);
    }

    pub fn delete_file(path: &str) {
        let path = JsValue::from_str(path);
        deleteFile(&path);
    }
    pub fn list_files(path: &str) {
        let path = JsValue::from_str(path);
        listFiles(&path);
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

fn main() {
    dioxus_logger::init(Level::INFO).expect("failed to init logger");
    info!("starting app");
    launch(App);
}

fn App() -> Element {
    use_context_provider(State::new);
    rsx! {
        Router::<Route> {}
    }
}

#[derive(Clone, Default)]
pub struct State {
    inner: Arc<Mutex<InnerState>>,
}

#[derive(Deserialize)]
struct GithubUser {
    login: String,
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
    let mut opts = RequestInit::new();
    opts.method("GET");
    opts.mode(RequestMode::Cors);

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

async fn load_cached_info() -> Option<UserInfo> {
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

impl State {
    pub fn new() -> Self {
        let selv = Self::default();
        selv
    }

    pub fn info(&self) -> Signal<Option<UserInfo>> {
        self.inner.lock().unwrap().token.clone()
    }
}

#[derive(Debug)]
struct UserInfo {
    auth_token: String,
    install_token: String,
    username: String,
}

#[derive(Default)]
struct InnerState {
    token: Signal<Option<UserInfo>>,
}

#[component]
fn Home() -> Element {
    let state = use_context::<State>();
    let state2 = state.clone();

    let mut repopath = use_signal(|| "/sup".to_string());

    let mut niceinfo = state.info();
    use_effect(move || {
        log_to_console("YY");
        spawn(async move {
            let new_info = load_cached_info().await;
            log_to_console(("EYYY", &new_info));
            niceinfo.set(new_info);
        });
    });

    let flag = state.info();

    rsx! {
        h1 {"state: {flag:?}"}
        button { onclick: move |_| {
            let state = state.clone();

            let mut info = state.info();
            spawn(async move {
                log_to_console("XX");
                let new_info = load_user_info().await;
                info.set(new_info);
            });

        }, "log in" }
        button { onclick:  |_|{
        }, "update lol" },
        button { onclick: move |_| {
            js::list_files(repopath().as_ref());
        }, "show repo!" }
        button { onclick: move |_| {
            if let Some(info) = flag.as_ref(){
                js::clone_repo(repopath().as_ref(), "https://github.com/tbs1996/remotespeki.git", &info.install_token);
            }

        }, "clone repo!" }

        button { onclick: move |_| {
            spawn(async move {
                for x in IndexBaseProvider.load_all_attributes().await {
                    let x = format!("{:?}", x) ;
                    log_to_console(&x);
                }
            });

        }, "load cards" }
        input {
            // we tell the component what to render
            value: "{repopath}",
            // and what to do when the value changes
            oninput: move |event| repopath.set(event.value())
        }
    }
}

use crate::provider::IndexBaseProvider;
