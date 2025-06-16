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
    dioxus_logger::init(Level::INFO).expect("failed to init logger");
    info!("starting speki");

    dioxus::launch(TheApp);
}

#[component]
pub fn TheApp() -> Element {
    let id = current_scope_id();
    info!("omg?? scope id: {id:?}");
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
    pub const BLACK_BUTTON: &'static str = "mt-2 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base md:mt-0";
    pub const BLUE_BUTTON: &'static str = "text-center py-4 px-6 bg-blue-500 text-white font-bold rounded-lg shadow hover:bg-blue-600 transition";
}
