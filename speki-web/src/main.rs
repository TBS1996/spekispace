#![allow(non_snake_case)]

use crate::pages::{Add, Browse, Home, Review};
use crate::utils::App;
use dioxus::prelude::*;
use dioxus_logger::tracing::{info, Level};
use login::LoginState;
use pages::BrowseState;
use review_state::ReviewState;

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
    speki_web::say_hello();

    dioxus_logger::init(Level::INFO).expect("failed to init logger");
    info!("starting app");

    dioxus::launch(|| {
        use_context_provider(App::new);
        use_context_provider(ReviewState::default);
        use_context_provider(LoginState::default);
        use_context_provider(BrowseState::new);

        spawn(async move {
            let rev = use_context::<App>();
            rev.0.fill_cache().await;
            speki_web::set_app(rev.0.clone());
        });

        rsx! {
            document::Link {
                rel: "stylesheet",
                href: asset!("/public/tailwind.css")
            }

            Router::<Route> {}
        }
    });
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
