#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_logger::tracing::{info, Level};
use login::LoginState;
use pages::add_card::AddCardState;
use pages::BrowseState;
use review_state::ReviewState;

use crate::pages::{Browse, Home, Review};
use crate::utils::App;

mod components;
mod firebase;
mod graph;
mod js;
mod login;
mod nav;
mod overlays;
mod pages;
mod review_state;
mod utils;

pub const REPO_PATH: &'static str = "/foobar";
pub const PROXY: &'static str = "http://127.0.0.1:8081";

pub const DEFAULT_FILTER: &'static str =
    "recall < 0.8 & finished == true & suspended == false & minrecrecall > 0.8 & lastreview > 0.5 & weeklapses < 3 & monthlapses < 6";

pub trait PopTray {
    fn is_done(&self) -> Signal<bool>;
    fn render(&self) -> Element;
}

pub type PopupEntry = Signal<Option<Popup>>;
pub type Popup = Box<dyn PopTray>;

#[derive(Clone, Default)]
pub struct OverlayManager {
    home: PopupEntry,
    review: PopupEntry,
    add: PopupEntry,
    browse: PopupEntry,
}

impl OverlayManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, popup: Popup) {
        self.get().clone().set(Some(popup));
    }

    pub fn render(&self) -> Option<Element> {
        let pop = self.get();
        let Some(pop) = pop.as_ref() else {
            return None;
        };

        if *pop.is_done().read() {
            None
        } else {
            Some(pop.render())
        }
    }

    pub fn get(&self) -> PopupEntry {
        let route = use_route::<Route>();
        info!("getting route popup..");
        match route {
            Route::Home {} => self.home.clone(),
            Route::Review {} => self.review.clone(),
            Route::Add {} => self.add.clone(),
            Route::Browse {} => self.browse.clone(),
        }
    }

    pub fn clear(&self) {
        self.get().clone().set(Default::default());
    }
}

fn main() {
    dioxus_logger::init(Level::INFO).expect("failed to init logger");
    info!("starting app");
    let id = current_scope_id();
    info!("lol scope id: {id:?}");

    dioxus::launch(TheApp);
}

#[component]
pub fn TheApp() -> Element {
    let id = current_scope_id();
    info!("top scope id: {id:?}");

    let app = use_context_provider(App::new);
    let addcard = AddCardState::new(app.clone());
    use_context_provider(|| addcard.clone());
    use_context_provider(|| ReviewState::new(app.clone()));
    use_context_provider(LoginState::default);
    use_context_provider(BrowseState::new);
    use_context_provider(OverlayManager::new);

    spawn(async move {
        app.0.fill_cache().await;
        speki_web::set_app(app.0.clone());
        addcard.load_cards().await;
    });

    rsx! {
        document::Link {
            rel: "stylesheet",
            href: asset!("/public/tailwind.css")
        }
        { info!("hey lol") }

        Router::<Route> {}
    }
}

#[component]
fn Wrapper() -> Element {
    info!("wrapper!!!!!!!");
    let id = current_scope_id();
    info!("wrapper scope id: {id:?}");

    rsx! {
         crate::nav::nav {}
         Outlet::<Route> {}

    }
}

use crate::pages::add_card::Add;

#[derive(Clone, Routable, Debug, PartialEq)]
pub enum Route {
    #[layout(Wrapper)]
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
