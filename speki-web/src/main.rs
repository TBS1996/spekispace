#![allow(non_snake_case)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use dioxus::prelude::*;
use dioxus_logger::tracing::{info, Level};
use utils::CardEntries;

use crate::pages::{Browse, Home, Review};
use crate::utils::App;
use components::GraphRep;
use pages::add_card::AddCardState;
use pages::{BrowseState, ReviewState};

//mod github;
mod components;
mod firebase;
mod js;
mod nav;
mod overlays;
mod pages;
mod utils;

pub const DEFAULT_FILTER: &'static str =
    "recall < 0.8 & finished == true & suspended == false & minrecrecall > 0.8 & lastreview > 0.5 & weeklapses < 3 & monthlapses < 6";

pub type PopupEntry = Signal<Vec<Arc<Popup>>>;
pub type Popup = Box<dyn PopTray>;

/// We need to re-render cyto instance every time the route changes, so this boolean
/// is true every time we change route, and is set back to false after the cyto instance is re-rendered
pub static ROUTE_CHANGE: AtomicBool = AtomicBool::new(false);

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
        self.get().clone().write().push(Arc::new(popup));
    }

    pub fn replace(&self, popup: Popup) {
        self.pop();
        self.get().clone().write().push(Arc::new(popup));
    }

    pub fn render(&self) -> Option<Element> {
        info!("render popup!");
        let pop = self.get_last_not_done()?;

        let mut done_signal = pop.is_done().clone();

        if done_signal() {
            None
        } else {
            Some(rsx! {
            button {
                class: "float-right mr-4 mb-10",
                onclick: move |_| {
                    done_signal.set(true);
                },
                "❌"
            },

            { pop.render() }
            })
        }
    }

    fn get_last(&self) -> Option<Arc<Popup>> {
        self.get().read().last().cloned()
    }

    fn get_last_not_done(&self) -> Option<Arc<Popup>> {
        loop {
            let last = self.get_last()?;
            if last.is_done().cloned() {
                self.pop().unwrap();
            } else {
                return Some(last);
            }
        }
    }

    fn get(&self) -> PopupEntry {
        let route = use_route::<Route>();
        info!("getting route popup..");
        match route {
            Route::Home {} => self.home.clone(),
            Route::Review {} => self.review.clone(),
            Route::Add {} => self.add.clone(),
            Route::Browse {} => self.browse.clone(),
        }
    }

    fn pop(&self) -> Option<Arc<Popup>> {
        self.get().write().pop()
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
    let graph = GraphRep::init(None);
    use_context_provider(|| ReviewState::new(app.clone(), graph));
    use_context_provider(OverlayManager::new);
    let graph2 = GraphRep::init(None);
    let entries = use_context_provider(CardEntries::default);
    let addcard = AddCardState::new(graph2, app.clone(), entries.clone());
    use_context_provider(|| addcard.clone());
    let browse_state = BrowseState::new(entries.clone());
    use_context_provider(|| browse_state);

    spawn(async move {
        app.0.fill_cache().await;
        entries.fill(app).await;
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
    info!("wrapper??!!!!!");
    ROUTE_CHANGE.store(true, Ordering::SeqCst);
    let id = current_scope_id();
    info!("wrapper scope id: {id:?}");
    let overlay = use_context::<OverlayManager>();

    rsx! {
         crate::nav::nav {}
         if let Some(overlay) = overlay.render() {
            { overlay }
         } else {
            Outlet::<Route> {}
         }

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

pub trait Komponent {
    fn render(&self) -> Element;
}

pub trait PopTray: Komponent {
    fn is_done(&self) -> Signal<bool>;
    fn set_done(&self) {
        self.is_done().clone().set(true);
    }
}
