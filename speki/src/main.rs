#![allow(non_snake_case)]

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use dioxus::prelude::*;
use dioxus_logger::tracing::{info, Level};
#[cfg(not(feature = "desktop"))]
use firebase::AuthUser;
use pages::{ImportState, ReviewPage};

use crate::{
    overlays::OverlayEnum,
    pages::{Add, Browse, Import, Review},
    utils::App,
};

mod components;
mod nav;
mod overlays;
mod pages;
mod utils;

/// We need to re-render cyto instance every time the route changes, so this boolean
/// is true every time we change route, and is set back to false after the cyto instance is re-rendered
pub static ROUTE_CHANGE: AtomicBool = AtomicBool::new(false);

const TAILWIND_CSS: &str = include_str!("../public/tailwind.css");

fn main() {
    std::env::set_var("GDK_BACKEND", "x11");

    let trace_enabled = std::env::args().any(|arg| arg == "--trace");
    let log_level = if trace_enabled {
        Level::DEBUG
    } else {
        Level::INFO
    };

    dioxus_logger::init(log_level).expect("failed to init logger");

    info!("starting speki");

    dioxus::launch(TheApp);
}

#[component]
pub fn TheApp() -> Element {
    use_context_provider(ImportState::new);
    use_context_provider(ReviewPage::new);

    rsx! {
        style { dangerous_inner_html: "{TAILWIND_CSS}" }

        div {
            class: "w-screen min-h-screen",
            Router::<Route> {}
        }

    }
}

static APP: GlobalSignal<App> = Signal::global(App::new);
static CURRENT_ROUTE: GlobalSignal<Route> = Signal::global(|| Route::Review {});
pub static OVERLAY: GlobalSignal<Overlays> = Signal::global(Default::default);

pub fn pop_overlay() {
    OVERLAY.write().pop();
}

pub fn append_overlay(overlay: OverlayEnum) {
    OVERLAY.write().append(overlay);
}

pub fn set_overlay(overlay: Option<OverlayEnum>) {
    OVERLAY.write().set(overlay);
}

#[derive(Debug, Default)]
pub struct Overlays {
    review: (Signal<Option<Arc<OverlayEnum>>>, Vec<Arc<OverlayEnum>>),
    add_cards: (Signal<Option<Arc<OverlayEnum>>>, Vec<Arc<OverlayEnum>>),
    browse: (Signal<Option<Arc<OverlayEnum>>>, Vec<Arc<OverlayEnum>>),
}

impl Overlays {
    pub fn get(&self) -> Signal<Option<Arc<OverlayEnum>>> {
        let route = CURRENT_ROUTE.cloned();

        match route {
            Route::Review {} => self.review.0.clone(),
            Route::Add {} => self.add_cards.0.clone(),
            Route::Browse {} => self.browse.0.clone(),
            _ => todo!(),
        }
    }

    pub fn set(&mut self, new_overlay: Option<OverlayEnum>) {
        let new_overlay = new_overlay.map(Arc::new);
        let route = CURRENT_ROUTE.cloned();

        match route {
            Route::Review {} => {
                self.review.0.set(new_overlay.clone());
                self.review.1.pop();
                self.review.1.extend(new_overlay);
            }
            Route::Add {} => {
                self.add_cards.0.set(new_overlay.clone());
                self.add_cards.1.pop();
                self.add_cards.1.extend(new_overlay);
            }
            Route::Browse {} => {
                self.browse.0.set(new_overlay.clone());
                self.browse.1.pop();
                self.browse.1.extend(new_overlay);
            }
            _ => todo!(),
        }
    }

    pub fn append(&mut self, new_overlay: OverlayEnum) {
        let new_overlay = Arc::new(new_overlay);
        let route = CURRENT_ROUTE.cloned();

        match route {
            Route::Review {} => {
                self.review.0.set(Some(new_overlay.clone()));
                self.review.1.push(new_overlay);
            }
            Route::Add {} => {
                self.add_cards.0.set(Some(new_overlay.clone()));
                self.add_cards.1.push(new_overlay);
            }
            Route::Browse {} => {
                self.browse.0.set(Some(new_overlay.clone()));
                self.browse.1.push(new_overlay);
            }
            _ => todo!(),
        }
    }

    pub fn pop(&mut self) {
        let route = CURRENT_ROUTE.cloned();

        match route {
            Route::Review {} => {
                self.review.1.pop();
                let new = self.review.1.last().cloned();
                self.review.0.set(new);
            }
            Route::Add {} => {
                self.add_cards.1.pop();
                let new = self.add_cards.1.last().cloned();
                self.add_cards.0.set(new);
            }
            Route::Browse {} => {
                self.browse.1.pop();
                let new = self.browse.1.last().cloned();
                self.browse.0.set(new);
            }
            _ => todo!(),
        }
    }
}

#[component]
fn Wrapper() -> Element {
    *CURRENT_ROUTE.write() = use_route::<Route>();
    info!("wrapper scope id: {:?}", current_scope_id());
    ROUTE_CHANGE.store(true, Ordering::SeqCst);

    rsx! {
        div {
            class: "h-screen overflow-hidden flex flex-col",
            crate::nav::nav {}

            div {
                class: "flex-1 overflow-hidden",
                Outlet::<Route> {}
            }
        }
    }
}

#[derive(Copy, Clone, Routable, Debug, PartialEq, Hash, Eq)]
pub enum Route {
    #[layout(Wrapper)]
    #[route("/")]
    Review {},
    #[route("/add")]
    Add {},
    #[route("/browse")]
    Browse {},
    #[route("/import")]
    Import {},
}

impl Route {
    pub fn label(&self) -> &'static str {
        match self {
            Route::Review {} => "review",
            Route::Add {} => "add cards",
            Route::Browse {} => "browse",
            Route::Import {} => "import",
        }
    }
}

pub mod styles {

    #[derive(Clone, PartialEq, Eq, Copy)]
    pub enum CRUD {
        Create,
        Read,
        Update,
        Delete,
    }

    impl CRUD {
        pub fn style(&self) -> &'static str {
            match self {
                CRUD::Create => CREATE_BUTTON,
                CRUD::Read => READ_BUTTON,
                CRUD::Update => UPDATE_BUTTON,
                CRUD::Delete => DELETE_BUTTON,
            }
        }
    }

    pub const BLACK_BUTTON: &'static str = "\
mt-2 inline-flex items-center text-white \
bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 \
disabled:bg-gray-400 disabled:cursor-not-allowed disabled:opacity-50 \
rounded text-base md:mt-0";

    pub const CREATE_BUTTON: &str = "\
mt-2 inline-flex items-center text-white \
bg-green-600 border-0 py-1 px-3 focus:outline-none hover:bg-green-700 \
disabled:bg-green-400 disabled:cursor-not-allowed disabled:opacity-50 \
rounded text-base md:mt-0";

    pub const READ_BUTTON: &str = "\
mt-2 inline-flex items-center text-white \
bg-blue-600 border-0 py-1 px-3 focus:outline-none hover:bg-blue-700 \
disabled:bg-blue-400 disabled:cursor-not-allowed disabled:opacity-50 \
rounded text-base md:mt-0";

    pub const UPDATE_BUTTON: &str = "\
mt-2 inline-flex items-center text-white \
bg-amber-600 border-0 py-1 px-3 focus:outline-none hover:bg-amber-700 \
disabled:bg-amber-400 disabled:cursor-not-allowed disabled:opacity-50 \
rounded text-base md:mt-0";

    pub const XS_UPDATE: &str = "\
mt-2 inline-flex items-center text-white \
bg-amber-600 border-0 py-0.5 px-2 focus:outline-none hover:bg-amber-700 \
disabled:bg-amber-400 disabled:cursor-not-allowed disabled:opacity-50 \
rounded text-sm md:mt-0";

    pub const DELETE_BUTTON: &str = "\
mt-2 inline-flex items-center text-white \
bg-red-600 border-0 py-1 px-3 focus:outline-none hover:bg-red-700 \
disabled:bg-red-400 disabled:cursor-not-allowed disabled:opacity-50 \
rounded text-base md:mt-0";
}
