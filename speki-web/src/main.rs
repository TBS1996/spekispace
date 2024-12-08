#![allow(non_snake_case)]

use components::backside::BackPut;
use dioxus::prelude::*;
use dioxus_logger::tracing::{info, Level};
use login::LoginState;
use pages::BrowseState;
use review_state::ReviewState;

use crate::pages::{Add, Browse, Home, Review};
use crate::utils::App;

mod components;
mod graph;
mod js;
mod login;
mod nav;
mod pages;
mod review_state;
mod utils;

pub const REPO_PATH: &'static str = "/foobar";
pub const PROXY: &'static str = "http://127.0.0.1:8081";

pub const DEFAULT_FILTER: &'static str =
    "recall < 0.8 & finished == true & suspended == false & minrecrecall > 0.8 & lastreview > 0.5 & weeklapses < 3 & monthlapses < 6";

fn main() {
    dioxus_logger::init(Level::INFO).expect("failed to init logger");
    info!("starting app");

    dioxus::launch(TheApp);
}

#[component]
pub fn TheApp() -> Element {
    let app = use_context_provider(App::new);
    let backput = BackPut::new(app.clone());
    use_context_provider(|| backput.clone());
    use_context_provider(|| ReviewState::new(app.clone()));
    use_context_provider(LoginState::default);
    use_context_provider(BrowseState::new);

    spawn(async move {
        app.0.fill_cache().await;
        speki_web::set_app(app.0.clone());
        backput.load_cards().await;
    });

    rsx! {
        document::Link {
            rel: "stylesheet",
            href: asset!("/public/tailwind.css")
        }

        Router::<Route> {}
    }
}

#[derive(Clone, Routable, Debug, PartialEq)]
pub enum Route {
    #[route("/")]
    Home {},
    #[route("/review")]
    Review {},
    #[route("/add")]
    Add {},
    #[route("/browse")]
    Browse {},
}

impl Route {
    pub fn label(&self) -> &'static str {
        match self {
            Route::Home {} => "home",
            Route::Review {} => "review",
            Route::Add {} => "add cards",
            Route::Browse {} => "browse",
        }
    }
}
