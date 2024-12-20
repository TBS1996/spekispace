#![allow(non_snake_case)]

use std::{
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
};

use components::GraphRep;
use dioxus::prelude::*;
use dioxus_logger::tracing::{info, Level};
use overlays::OverlayManager;
use pages::{add_card::AddCardState, BrowseState, ReviewState};
use utils::CardEntries;

use crate::{
    pages::{add_card::Add, Browse, Home, Review},
    utils::App,
};

//mod github;
mod components;
mod firebase;
mod nav;
mod overlays;
mod pages;
mod utils;

pub const DEFAULT_FILTER: &'static str =
    "recall < 0.8 & finished == true & suspended == false & minrecrecall > 0.8 & lastreview > 0.5 & weeklapses < 3 & monthlapses < 6";

/// We need to re-render cyto instance every time the route changes, so this boolean
/// is true every time we change route, and is set back to false after the cyto instance is re-rendered
pub static ROUTE_CHANGE: AtomicBool = AtomicBool::new(false);

fn main() {
    dioxus_logger::init(Level::INFO).expect("failed to init logger");
    info!("starting app");
    let id = current_scope_id();
    info!("super very scope id: {id:?}");

    dioxus::launch(TheApp);
}

static CARDS: GlobalSignal<CardEntries> = Signal::global(CardEntries::default);
static BROWSE_STATE: GlobalSignal<BrowseState> = Signal::global(BrowseState::new);
static APP: GlobalSignal<App> = Signal::global(App::new);
static OVERLAY: GlobalSignal<OverlayManager> = Signal::global(OverlayManager::new);
static REVIEW_STATE: GlobalSignal<ReviewState> =
    Signal::global(|| ReviewState::new(GraphRep::default()));
static ADD_CARDS: GlobalSignal<AddCardState> =
    Signal::global(|| AddCardState::new(GraphRep::default()));

#[component]
pub fn TheApp() -> Element {
    let id = current_scope_id();
    info!("top scope id: {id:?}");

    spawn(async move {
        APP.read().0.fill_cache().await;
        CARDS.cloned().fill().await;
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
    info!("xd wrapper??!!!!!");
    ROUTE_CHANGE.store(true, Ordering::SeqCst);
    info!("wrapper scope id: {:?}", current_scope_id().unwrap());
    let overlay = OVERLAY.cloned();

    let log_event = move |event: Rc<KeyboardData>| {
        if let Key::Escape = event.key() {
            OVERLAY.read().pop();
            info!("escape!!");
        }
        info!("event is: {event:?}");
    };

    rsx! {
        div {
        id: "receiver",
        tabindex: 0,
        onkeydown: move |event| log_event(event.data()),

         crate::nav::nav {}
        {info!("rsx scope id: {:?}", current_scope_id().unwrap());}
         if let Some(overlay) = overlay.render() {
            { overlay }
         } else {
            Outlet::<Route> {}
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
}
