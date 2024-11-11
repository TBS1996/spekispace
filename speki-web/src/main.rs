#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_logger::tracing::{info, Level};
use gloo_net::http::Request;
use serde::Deserialize;
use std::sync::{Arc, Mutex};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use web_sys::console;

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

use js_sys::Promise;
use wasm_bindgen_futures::JsFuture;

fn get_auth_token() -> Option<String> {
    cookies::get("auth-token")
}

pub fn log_to_console(message: impl std::fmt::Debug) {
    let message = format!("{:?}", message);
    console::log_1(&JsValue::from_str(&message));
}

#[derive(Clone, Routable, Debug, PartialEq)]
enum Route {
    #[route("/")]
    Home {},
}

#[wasm_bindgen(module = "/assets/utils.js")]
extern "C" {
    fn greet(name: &str);
    fn clone_repo_and_list_files();
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
use futures::executor::block_on;

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

impl State {
    pub fn new() -> Self {
        greet("yo whassup");
        let selv = Self::default();
        selv
    }

    pub fn info(&self) -> Signal<Option<UserInfo>> {
        self.inner.lock().unwrap().token.clone()
    }

    pub fn load_token(&self) {
        let self_clone = self.clone();
        use_effect(move || {
            log_to_console("loadin token!!");

            let value = self_clone.clone();
            spawn_local(async move {
                let mut token = value.info();
                if let Some(auth) = get_auth_token() {
                    let username = fetch_github_username(auth.clone()).await.unwrap();
                    let info = UserInfo {
                        username,
                        token: auth,
                    };
                    token.set(Some(info));
                }
            });
        });
    }
}

#[derive(Debug)]
struct UserInfo {
    token: String,
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

    let mut flag = state.info();
    let x = get_auth_token();
    log_to_console(("cookies:", x));

    rsx! {
        h1 {"state: {flag:?}"}


        button { onclick: move |_| {
            let state = state.clone();

            spawn_local(async move {
                let auth_url = "http://localhost:3000/auth/github";
                let x = web_sys::window().unwrap().location().set_href(auth_url).unwrap();
            });

        }, "log in" }
        button { onclick: move |_|{
            let state = state2.clone();


        }, "update lol" },


        button { onclick: move |_| {
            clone_repo_and_list_files();
        }, "repo stuff" }
    }
}
