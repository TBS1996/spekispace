#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_logger::tracing::{info, Level};
use login::LoginState;
use review_state::ReviewState;
use speki_core::TimeProvider;
use std::{sync::Arc, time::Duration};
use wasm_bindgen::prelude::*;
use web_sys::console;

use crate::pages::{Add, Debug, Home, Review, View};
use crate::provider::IndexBaseProvider;

mod js;
mod login;
mod nav;
mod pages;
mod provider;
mod review_state;
mod utils;

pub const REPO_PATH: &'static str = "/foobar";
pub const PROXY: &'static str = "http://127.0.0.1:8081";

#[derive(Clone)]
pub struct App(pub Arc<speki_core::App>);

impl App {
    fn new() -> Self {
        Self(Arc::new(speki_core::App::new(
            IndexBaseProvider::new(REPO_PATH),
            speki_core::SimpleRecall,
            WasmTime,
        )))
    }
}

fn main() {
    dioxus_logger::init(Level::INFO).expect("failed to init logger");
    info!("starting app");

    dioxus::launch(|| {
        use_context_provider(App::new);
        use_context_provider(ReviewState::default);
        use_context_provider(LoginState::default);

        rsx! {
            document::Link {
                rel: "stylesheet",
                href: asset!("/public/tailwind.css")
            }

            Router::<Route> {}
        }
    });
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
    #[route("/view/:id")]
    View { id: String },
    #[route("/add")]
    Add {},
    #[route("/debug")]
    Debug {},
}

struct WasmTime;

impl TimeProvider for WasmTime {
    fn current_time(&self) -> Duration {
        js::current_time()
    }
}
