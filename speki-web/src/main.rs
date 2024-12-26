#![allow(non_snake_case)]

use std::{
    collections::HashMap,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use dioxus::prelude::*;
use dioxus_logger::tracing::{info, Level};
use overlays::OverlayManager;
use utils::CardEntries;

use crate::{
    pages::{add_card::Add, Browse, Home, Review},
    utils::App,
};

mod components;
mod firebase;
mod nav;
mod overlays;
mod pages;
mod utils;

pub const DEFAULT_FILTER: &'static str =
    "recall < 0.8 & finished == true & suspended == false & minrecrecall > 0.8 & minrecstab > 50 & lastreview > 0.5 & weeklapses < 3 & monthlapses < 6";

/// We need to re-render cyto instance every time the route changes, so this boolean
/// is true every time we change route, and is set back to false after the cyto instance is re-rendered
pub static ROUTE_CHANGE: AtomicBool = AtomicBool::new(false);

fn main() {
    dioxus_logger::init(Level::INFO).expect("failed to init logger");
    info!("starting app");
    let id = current_scope_id();
    info!("omg very scope id: {id:?}");

    dioxus::launch(TheApp);
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

#[derive(Clone, Debug, Default)]
pub struct NonClickable {
    inner: Arc<Mutex<HashMap<Route, HashMap<String, TouchRec>>>>,
}

impl NonClickable {
    pub fn contains(&self, point: Point) -> bool {
        let route = CURRENT_ROUTE.cloned();
        for (id, rec) in self.inner.lock().unwrap().entry(route).or_default().iter() {
            if rec.contains(point) {
                if utils::is_element_present(id) {
                    return true;
                }
            }
        }

        false
    }

    pub fn clear(&self) {
        let route = CURRENT_ROUTE.cloned();
        self.inner.lock().unwrap().entry(route).or_default().clear();
    }

    pub fn insert(&self, id: String, rec: TouchRec) {
        let route = CURRENT_ROUTE.cloned();
        self.inner
            .lock()
            .unwrap()
            .entry(route)
            .or_default()
            .insert(id, rec);
    }
}

static CARDS: GlobalSignal<CardEntries> = Signal::global(CardEntries::default);
static APP: GlobalSignal<App> = Signal::global(App::new);
static OVERLAY: GlobalSignal<OverlayManager> = Signal::global(Default::default);
static NONCLICKABLE: GlobalSignal<NonClickable> = Signal::global(Default::default);
static IS_SHORT: GlobalSignal<bool> = Signal::global(|| screen_height_in_inches().unwrap() < 4.);
static CURRENT_ROUTE: GlobalSignal<Route> = Signal::global(|| Route::Home {});

#[component]
pub fn TheApp() -> Element {
    let id = current_scope_id();
    info!("omg?? scope id: {id:?}");

    spawn(async move {
        APP.read().fill_cache().await;
        CARDS.cloned().fill().await;
    });

    rsx! {
        document::Link {
            rel: "stylesheet",
            href: asset!("/public/tailwind.css")
        }

        div {
            class: "bg-white min-h-screen",
            Router::<Route> {}
        }

    }
}

/// Estimates the screen height in inches.
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
    info!("wrapper scope id: {:?}", current_scope_id().unwrap());
    ROUTE_CHANGE.store(true, Ordering::SeqCst);
    let overlay = OVERLAY.cloned();

    let log_event = move |event: Rc<KeyboardData>| {
        if let Key::Escape = event.key() {
            OVERLAY.read().pop();
        }
    };

    rsx! {
        div {
            id: "receiver",
            tabindex: 0,
            onkeydown: move |event| log_event(event.data()),
            class: "h-screen overflow-hidden flex flex-col",

            if !IS_SHORT() {
                crate::nav::nav {}

                div {
                    class: "flex-1 overflow-hidden",
                    if let Some(overlay) = overlay.render() {
                        { overlay }
                    } else {
                        Outlet::<Route> {}
                    }
                }
            }

            if IS_SHORT() {
                div {
                    class: "flex-1 overflow-hidden pb-8",
                    if let Some(overlay) = overlay.render() {
                        { overlay }
                    } else {
                        Outlet::<Route> {}
                    }
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

    fn left(&self) -> Option<Self> {
        match self {
            Route::Home {} => None,
            Route::Review {} => None,
            Route::Add {} => Some(Self::Review {}),
            Route::Browse {} => Some(Self::Add {}),
        }
    }

    fn right(&self) -> Option<Self> {
        match self {
            Route::Home {} => None,
            Route::Review {} => Some(Self::Add {}),
            Route::Add {} => Some(Self::Browse {}),
            Route::Browse {} => None,
        }
    }
}
