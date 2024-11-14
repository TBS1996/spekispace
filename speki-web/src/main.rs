#![allow(non_snake_case)]

use crate::provider::IndexBaseProvider;
use dioxus::prelude::*;
use dioxus_logger::tracing::{info, Level};
use serde::Deserialize;
use speki_core::{AnyType, App, Card, TimeProvider};
use speki_dto::{CardId, Recall, Review, SpekiProvider};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use wasm_bindgen::prelude::*;
use web_sys::console;

mod provider;
mod utils;

const DEFAULT_FILTER: &'static str =
    "recall < 0.8 & finished == true & suspended == false & resolved == true & minrecrecall > 0.8 & minrecstab > 10 & lastreview > 0.5 & weeklapses < 3 & monthlapses < 6";

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
    #[route("/review")]
    Review {},
}

pub mod js {
    use std::time::Duration;

    use gloo_utils::format::JsValueSerdeExt;
    use js_sys::Promise;
    use serde_json::Value;
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = Date)]
        fn now() -> f64;
    }

    #[wasm_bindgen(module = "/assets/utils.js")]
    extern "C" {
        fn loadAllFiles(path: &JsValue) -> Promise;
        fn cloneRepo(path: &JsValue, url: &JsValue, token: &JsValue);
        fn listFiles(path: &JsValue);
        fn deleteFile(path: &JsValue);
        fn loadFile(path: &JsValue) -> Promise;
        fn saveFile(path: &JsValue, content: &JsValue);
    }

    pub fn current_time() -> Duration {
        Duration::from_millis(now() as u64)
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

#[derive(Clone)]
pub struct State {
    inner: Arc<Mutex<InnerState>>,
    app: Arc<App>,
}

impl State {
    pub fn new() -> Self {
        let app = App::new(IndexBaseProvider, speki_core::SimpleRecall, WasmTime);
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
        self.next_card(app).await;
    }

    async fn make_review(&self, review: Review) {
        if let Some(id) = self.id() {
            IndexBaseProvider.add_review(id, review).await;
        }
    }

    fn current_pos(&self) -> usize {
        self.tot_len - self.queue.lock().unwrap().len()
    }

    async fn do_review(&self, app: &App, review: Review) {
        self.make_review(review).await;
        self.next_card(app).await;
    }

    async fn next_card(&self, app: &App) {
        let card = self.queue.lock().unwrap().pop();

        let card = match card {
            Some(id) => {
                let card = Card::from_raw(
                    app.foobar.clone(),
                    IndexBaseProvider.load_card(id).await.unwrap(),
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

fn new_review(recall: Recall) -> Review {
    Review {
        timestamp: js::current_time(),
        grade: recall,
        time_spent: Duration::default(),
    }
}

#[component]
fn Review() -> Element {
    let state = use_context::<State>();
    let review = use_context::<ReviewState>();
    let card = review.card.clone();
    let pos = review.pos.clone();
    let tot = review.tot_len.clone();
    let mut show_backside = use_signal(|| false);

    let front = review.front.clone();
    let back = review.back.clone();

    rsx! {
        div {
            match card() {
                Some(_) => rsx! {
                    div {
                        h2 { "Reviewing Card {pos} of {tot}" }
                        p { "Front: {front}" }
                        if show_backside() {
                            p { "Back: {back}" }
                            div {
                                button {
                                    onclick: move |_| {
                                        let review = review.clone();
                                        let state = state.clone();
                                        spawn(async move{
                                            review.do_review(&state.app, new_review(Recall::None)).await;
                                        });
                                    },
                                    "Easy"
                                }
                                button {
                                    onclick: move |_| {
                                        spawn(async move{
                                            let state = use_context::<State>();
                                            let review = use_context::<ReviewState>();
                                            review.do_review(&state.app, new_review(Recall::Late)).await;
                                        });
                                    },
                                    "Good"
                                }
                                button {
                                    onclick: move |_| {
                                        spawn(async move{
                                            let state = use_context::<State>();
                                            let review = use_context::<ReviewState>();
                                            review.do_review(&state.app, new_review(Recall::Some)).await;
                                        });
                                    },
                                    "Hard"
                                }
                                button {
                                    onclick: move |_| {
                                        spawn(async move{
                                            let state = use_context::<State>();
                                            let review = use_context::<ReviewState>();
                                            review.do_review(&state.app, new_review(Recall::Perfect)).await;
                                        });
                                    },
                                    "Again"
                                }
                            }
                        } else {
                            button {
                                onclick: move |_| show_backside.set(true),
                                "show backside"
                            }
                        }
                    }
                },

                // If there's no card, display the "Start Review" button
                None => rsx! {
                    div {
                        p { "No cards to review." }
                        button {
                            onclick: move |_| {
                                spawn(
                                    async move {
                                        let state = use_context::<State>();
                                        let review = use_context::<ReviewState>();
                                        review.refresh(&state.app, DEFAULT_FILTER.to_string()).await;
                                    }
                                );

                            },
                            "Start Review"
                        }
                    }
                },
            }
        }

    }
}

#[component]
fn Home() -> Element {
    let state = use_context::<State>();

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
