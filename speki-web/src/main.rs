#![allow(non_snake_case)]

use std::{
    env, fs,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use async_openai::{config::OpenAIConfig, types::CreateCompletionRequestArgs};
use dioxus::prelude::*;
use dioxus_logger::tracing::{info, Level};
#[cfg(not(feature = "desktop"))]
use firebase::AuthUser;
use ledgerstore::{LedgerEntry, TheLedgerEvent};
use pages::{ImportState, ReviewPage};
use serde::Deserialize;
use speki_core::{
    card::{CardId, RawCard},
    ledger::{CardAction, CardEvent},
    recall_rate::{Recall, Review as Xreview, ReviewAction, ReviewEvent},
    Card,
};

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

fn fix_history() {
    #[derive(Deserialize, Debug)]
    struct Event {
        id: CardId,
        grade: Recall,
        timestamp: Duration,
    }
    #[derive(Deserialize, Debug)]
    struct Record {
        index: usize,
        previous: Option<String>,
        event: Event,
    }

    let mut recs = vec![];
    for (idx, entry) in std::fs::read_dir("/home/tor/spekifs/snap4/history/entries")
        .unwrap()
        .into_iter()
        .enumerate()
    {
        let entry = entry.unwrap();
        if idx % 100 == 0 {
            dbg!(idx);
        }
        let s = std::fs::read_to_string(entry.path()).unwrap();

        let record: Record = match serde_json::from_str(&s) {
            Ok(r) => r,
            Err(e) => {
                dbg!(e);
                print!("{s}");
                panic!();
            }
        };
        recs.push(record);
    }

    recs.sort_by_key(|x| x.index);

    let path = PathBuf::from("/home/tor/spekifs/snap4/history/newentries");
    for record in recs {
        let event = record.event;
        let review = Xreview {
            timestamp: event.timestamp,
            grade: event.grade,
        };
        let event = ReviewEvent::new(event.id, ReviewAction::Insert(review));
        let entry = LedgerEntry {
            previous: record.previous,
            index: record.index,
            event,
        };

        entry.save(&path);
    }
}

fn fix_lol() {
    #[derive(Deserialize, Debug)]
    struct Event {
        action: Vec<CardAction>,
        id: CardId,
    }
    #[derive(Deserialize, Debug)]
    struct Record {
        index: usize,
        previous: Option<String>,
        event: Event,
    }

    let mut recs = vec![];
    for (idx, entry) in std::fs::read_dir("/home/tor/spekifs/snap4/rawcard/entries")
        .unwrap()
        .into_iter()
        .enumerate()
    {
        let entry = entry.unwrap();
        if idx % 100 == 0 {
            dbg!(idx);
        }
        let s = std::fs::read_to_string(entry.path()).unwrap();

        let record: Record = match serde_json::from_str(&s) {
            Ok(r) => r,
            Err(e) => {
                dbg!(e);
                print!("{s}");
                panic!();
            }
        };
        recs.push(record);
    }

    recs.sort_by_key(|x| x.index);

    let path = PathBuf::from("/home/tor/spekifs/snap4/rawcard/newentries");
    for record in recs {
        let mut event = record.event;
        let event = CardEvent::new(event.id, event.action.remove(0));
        let entry = LedgerEntry {
            previous: record.previous,
            index: record.index,
            event,
        };

        entry.save(&path);
    }
}

pub use async_openai::Client;

pub const DEFAULT_FILTER: &'static str =
    "recall < 0.8 & finished == true & suspended == false & minrecrecall > 0.8 & minrecstab > 50 & lastreview > 0.5 & weeklapses < 3 & monthlapses < 6";

/// We need to re-render cyto instance every time the route changes, so this boolean
/// is true every time we change route, and is set back to false after the cyto instance is re-rendered
pub static ROUTE_CHANGE: AtomicBool = AtomicBool::new(false);

fn load_api_key() -> Option<String> {
    let from_env = env::var("OPENAI_API_KEY");
    if from_env.is_ok() {
        return from_env.ok();
    }

    Some(
        fs::read_to_string("/home/tor/.secret/openai")
            .ok()?
            .trim()
            .to_owned(),
    )
}

/*
pub async fn ask_openai_card(card: Card) -> String {
    let provider = APP.read().inner().card_provider.clone();

    let namespace: Option<String> = match card.namespace() {
        Some(id) => Some(provider.load(id).unwrap().front_side().to_string()),
        None => None,
    };

    match card.clone_base().data.clone() {
        speki_core::CardType::Instance { name, class, .. } => {
            let class = provider.load(class).unwrap();
            let mut s = format!(
                "this card is an instance of the class: {}.",
                class.front_side().to_string()
            );

            for parent_class in class.parent_classes() {
                let parent_class = provider.load(parent_class).unwrap();
                let x = format!(
                    "which is a subclass of: {}",
                    parent_class.front_side().to_string()
                );
                s.push_str(&x);
            }
        }
        speki_core::CardType::Normal { front, back } => {},
        speki_core::CardType::Unfinished { front } => {},
        speki_core::CardType::Attribute {
            attribute,
            back,
            instance,
        } => {

        }
        speki_core::CardType::Class {
            name, parent_class, ..
        } => {
            let mut s = format!("this card is a class type. ");

            for parent_class in card.parent_classes() {
                let parent_class = provider.load(parent_class).unwrap();
                let x = format!(
                    "which is a subclass of: {}",
                    parent_class.front_side().to_string()
                );
                s.push_str(&x);
            }
        }
        speki_core::CardType::Statement { front } => todo!(),
        speki_core::CardType::Event {
            front,
            start_time,
            end_time,
            parent_event,
        } => todo!(),
    }

    let config = OpenAIConfig::new().with_api_key(load_api_key());
    let client = Client::with_config(config);

    let prefix: &'static str = "you are a flashcard assistant.
Answer the user's prompt with the shortest accurate answer possible — one fact, name, or definition. Never explain or elaborate.
Do not give examples to explain further. keep it very succinct.

If the prompt is not a question but simply hte name of a concept or thing in general, simply define the thing.

user prompt:
";

    let prompt = format!("{} {}", prefix, prompt.into());

    let request = CreateCompletionRequestArgs::default()
        .model("gpt-3.5-turbo-instruct")
        .prompt(prompt)
        .max_tokens(40_u32)
        .build()
        .unwrap();

    let response = client.completions().create(request).await.unwrap();

    response.choices.first().unwrap().text.clone()
}
*/

pub async fn ask_openai(key: String, prompt: impl Into<String>) -> String {
    let config = OpenAIConfig::new().with_api_key(key);
    let client = Client::with_config(config);

    let prefix: &'static str = "you are a flashcard assistant. 
Answer the user's prompt with the shortest accurate answer possible — one fact, name, or definition. Never explain or elaborate. 
Do not give examples to explain further. keep it very succinct.

If the prompt is not a question but simply hte name of a concept or thing in general, simply define the thing.

user prompt: 
";

    let prompt = format!("{} {}", prefix, prompt.into());

    let request = CreateCompletionRequestArgs::default()
        .model("gpt-3.5-turbo-instruct")
        .prompt(prompt)
        .max_tokens(40_u32)
        .build()
        .unwrap();

    let response = client.completions().create(request).await.unwrap();

    response.choices.first().unwrap().text.clone()
}

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

pub mod styles {
    pub const BLACK_BUTTON: &'static str = "mt-2 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base md:mt-0";
    pub const BLUE_BUTTON: &'static str = "text-center py-4 px-6 bg-blue-500 text-white font-bold rounded-lg shadow hover:bg-blue-600 transition";
}
