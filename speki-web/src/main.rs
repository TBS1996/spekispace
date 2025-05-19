#![allow(non_snake_case)]

use std::sync::atomic::{AtomicBool, Ordering};

use dioxus::prelude::*;
use dioxus_logger::tracing::{info, Level};
#[cfg(not(feature = "desktop"))]
use firebase::AuthUser;
use pages::{ImportState, ReviewPage};

use crate::{
    pages::{About, Add, Browse, Import, Menu, Review},
    utils::App,
};

mod components;
#[cfg(feature = "web")]
mod firebase;
mod nav;
mod overlays;
mod pages;
mod utils;

/*


*/

pub const DEFAULT_FILTER: &'static str =
    "recall < 0.8 & finished == true & suspended == false & minrecrecall > 0.8 & minrecstab > 50 & lastreview > 0.5 & weeklapses < 3 & monthlapses < 6";

/// We need to re-render cyto instance every time the route changes, so this boolean
/// is true every time we change route, and is set back to false after the cyto instance is re-rendered
pub static ROUTE_CHANGE: AtomicBool = AtomicBool::new(false);

use dioxus::desktop::{Config, LogicalSize, WindowBuilder};

fn main() {
    dioxus_logger::init(Level::INFO).expect("failed to init logger");
    info!("starting app");
    let id = current_scope_id();
    info!("omg very scope id: {id:?}");

    dioxus::launch(TheApp);
}

#[component]
fn TestApp() -> Element {
    let selected_text = use_signal(|| "nothing yet".to_string());

    rsx! {
        div {
            onmouseup: move |_| {
                let mut selected_text = selected_text.clone();
                spawn(async move {
                    let mut eval = document::eval(r#"
                        const sel = window.getSelection();
                        dioxus.send(sel ? sel.toString() : "NO_SELECTION");
                    "#);

                    if let Ok(val) = eval.recv::<String>().await {
                        selected_text.set(val);
                    }
                });
            },
            "Select some text in this box.",
            p {
                "You selected: {selected_text}"
            }
        }
    }
}

#[component]
pub fn TheApp() -> Element {
    let id = current_scope_id();
    info!("omg?? scope id: {id:?}");
    use_context_provider(ImportState::new);
    use_context_provider(ReviewPage::new);

    spawn(async move {
        #[cfg(not(feature = "desktop"))]
        {
            if let Some(currauth) = firebase::current_sign_in().await {
                *LOGIN_STATE.write() = Some(currauth);
                info!("user logged in!");
            } else {
                info!("no user logged in!");
            }
        }

        APP.read().fill_cache().await;
    });

    rsx! {
        document::Link {
            rel: "stylesheet",
            href: asset!("/public/tailwind.css")
        }

        div {
            class: "w-screen min-h-screen",
            Router::<Route> {}
        }

    }
}

#[derive(Debug, Copy, Clone)]
pub struct TouchRec {
    pub x: f64,
    pub y: f64,
    pub height: f64,
    pub width: f64,
}

impl TouchRec {
    pub fn contains(&self, point: Point) -> bool {
        point.x > self.x
            && point.x < (self.x + self.width)
            && point.y > self.y
            && point.y < (self.y + self.height)
    }
}

#[derive(Debug, Copy, Clone)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

static APP: GlobalSignal<App> = Signal::global(App::new);
static IS_SHORT: GlobalSignal<bool> = Signal::global(|| screen_height_in_inches().unwrap() < 4.);
static CURRENT_ROUTE: GlobalSignal<Route> = Signal::global(|| Route::Menu {});

#[cfg(not(feature = "desktop"))]
static LOGIN_STATE: GlobalSignal<Option<AuthUser>> = Signal::global(|| None);

/// Estimates the screen height in inches.
#[cfg(feature = "desktop")]
pub fn screen_height_in_inches() -> Option<f64> {
    Some(5.0)
}

/// Estimates the screen height in inches.
#[cfg(feature = "web")]
pub fn screen_height_in_inches() -> Option<f64> {
    let window = web_sys::window()?; // Access the browser window
    let screen = window.screen().unwrap(); // Access the screen object
    let height_pixels = screen.height().unwrap_or_default() as f64; // Screen height in CSS pixels
    let device_pixel_ratio = window.device_pixel_ratio(); // Get DPR
    let dpi = 96.0; // Assume 96 DPI as a baseline for most devices
    Some(height_pixels / (device_pixel_ratio * dpi)) // Calculate physical size
}

#[component]
fn Wrapper() -> Element {
    *CURRENT_ROUTE.write() = use_route::<Route>();
    info!("wrapper scope id: {:?}", current_scope_id());
    ROUTE_CHANGE.store(true, Ordering::SeqCst);

    rsx! {
        div {
            class: "h-screen overflow-hidden flex flex-col",

            if !IS_SHORT() {
                crate::nav::nav {}

                div {
                    class: "flex-1 overflow-hidden",
                    Outlet::<Route> {}
                }
            }

            if IS_SHORT() {
                div {
                    class: "flex-1 overflow-hidden",
                    Outlet::<Route> {}
                }

                crate::nav::nav {}
            }
        }
    }
}

#[derive(Copy, Clone, Routable, Debug, PartialEq, Hash, Eq)]
pub enum Route {
    #[layout(Wrapper)]
    #[route("/")]
    Menu {},
    #[route("/review")]
    Review {},
    #[route("/add")]
    Add {},
    #[route("/browse")]
    Browse {},
    #[route("/about")]
    About {},
    #[route("/import")]
    Import {},
    #[route("/debug")]
    Debug {},
}

impl Route {
    pub fn label(&self) -> &'static str {
        match self {
            Route::Menu {} => "menu",
            Route::Review {} => "review",
            Route::Add {} => "add cards",
            Route::Browse {} => "browse",
            Route::About {} => "about",
            Route::Import {} => "import",
            Route::Debug {} => "debug",
        }
    }
}

#[component]
fn Debug() -> Element {
    let card_hash = use_resource(move || async move {
        let hash = APP
            .read()
            .inner()
            .provider
            .cards
            .state_hash()
            .unwrap_or_default();
        hash
    });

    let review_hash = use_resource(move || async move {
        let hash = APP
            .read()
            .inner()
            .provider
            .reviews
            .state_hash()
            .unwrap_or_default();
        hash
    });

    rsx! {
        div {
            class: "flex flex-col",

            p {"cards hash: {card_hash.cloned().unwrap_or_default()}"}
            p {"history hash: {review_hash.cloned().unwrap_or_default()}"}
    }


        }
}
