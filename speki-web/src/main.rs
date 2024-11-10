#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_logger::tracing::{info, Level};
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
    console.log('Cookies:', cookies); // Log cookies to the console for debugging
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

fn main() {
    // Init logger
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

impl State {
    pub fn new() -> Self {
        // let cookie = block_on_get_cookie("auth-token");
        //log_to_console(cookie);
        let selv = Self::default();
        selv.load_token();
        selv
    }

    pub fn token(&self) -> Signal<Option<String>> {
        self.inner.lock().unwrap().token.clone()
    }

    pub fn load_token(&self) {
        let mut token = self.token();
        if let Some(auth) = get_auth_token() {
            token.set(Some(auth));
        }
    }
}

#[derive(Default)]
struct InnerState {
    token: Signal<Option<String>>,
}

#[component]
fn Home() -> Element {
    let state = use_context::<State>();

    let mut flag = state.token();
    let x = get_auth_token();
    log_to_console(("cookies:", x));

    rsx! {
        h1 {"state: {flag:?}"}

        button { onclick: move |_| {
                    let state = state.clone();

            spawn_local(async move {
                    let state = state.clone();
                    let auth_url = "http://localhost:3000/auth/github";
                    let x = web_sys::window().unwrap().location().set_href(auth_url).unwrap();
                    state.load_token();
            });

        }, "log in" }
        button { onclick: move |_| flag.set(None), "log out" }
    }
}
