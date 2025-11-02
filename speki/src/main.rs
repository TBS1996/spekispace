#![allow(non_snake_case)]

use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use clap::Parser;
use dioxus::prelude::*;
use dioxus_logger::tracing::{info, Level};
use ledgerstore::Ledger;
use pages::ReviewPage;
use speki_core::{
    card::{BackSide, CardId, RawCard, TextData},
    ledger::{CardAction, CardEvent},
    log_loss_accuracy,
    recall_rate::{
        ml::classic::Trained, AvgRecall, History, Recall, Review as TheReview, ReviewAction,
        ReviewEvent, FSRS,
    },
    set::{Input, Set, SetAction, SetEvent},
    Config, RecallChoice, SimpleRecall,
};
use std::fs;

use crate::{
    overlays::OverlayEnum,
    pages::{Add, Browse, Review},
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

#[derive(Parser, Debug)]
struct Cli {
    #[arg(long)]
    commit: Option<String>,
    #[arg(long)]
    debug_persist: bool,
    #[arg(long)]
    debug: bool,
    #[arg(long)]
    trace: bool,
    #[arg(long)]
    remote: bool,
    #[arg(long)]
    disable_remote: bool,
    #[arg(long)]
    import_cards: Option<PathBuf>,
    #[arg(long, num_args = 2)]
    add: Option<Vec<String>>,
    #[arg(long)]
    view_front: Option<CardId>,
    #[arg(long)]
    view_back: Option<CardId>,
    #[arg(long)]
    card: Option<CardId>,
    #[arg(long)]
    grade: Option<char>,
    #[arg(long)]
    analyze: bool,
    #[arg(long)]
    find_duplicates: bool,
    #[arg(long)]
    plot: Option<CardId>,
    #[arg(long)]
    maturity: Option<CardId>,
    #[arg(long)]
    generate_config: bool,
    #[arg(long)]
    review: bool,
    #[arg(long)]
    rebuild_state: bool,
}

#[derive(Clone)]
pub struct RemoteUpdate {
    new_remote_commit: Signal<Option<String>>,
}

impl RemoteUpdate {
    pub fn new() -> Self {
        let new_remote_commit: Signal<Option<String>> = Signal::new_in_scope(None, ScopeId::APP);
        let cloned = new_remote_commit.clone();

        spawn(async move {
            let current_commit = APP.read().current_commit();
            let latest_commit = APP.read().latest_upstream_commit();

            if latest_commit != current_commit {
                cloned.clone().set(latest_commit);
            }
        });

        Self { new_remote_commit }
    }

    pub fn latest_commit(&self) -> Option<String> {
        self.new_remote_commit.cloned()
    }

    pub fn clear(&self) {
        self.new_remote_commit.clone().set(None);
    }
}

fn main() {
    std::env::set_var("GDK_BACKEND", "x11");

    let cli = Cli::parse();

    let mut log_level = if cli.trace { Level::DEBUG } else { Level::INFO };

    let headless = cli.add.is_some()
        || cli.view_back.is_some()
        || cli.view_front.is_some()
        || cli.commit.is_some()
        || cli.import_cards.is_some()
        || cli.grade.is_some();

    if headless {
        log_level = Level::ERROR;
    }

    dioxus_logger::init(log_level).expect("failed to init logger");

    if cli.review {
        let path = Config::load().storage_path.clone();
        let app = speki_core::App::new(path);
        app.review_cli();
        return;
    } else if cli.rebuild_state {
        let path = Config::load().storage_path.clone();
        let cards: Ledger<RawCard> = Ledger::new_no_apply(path);

        use simpletime::timed;

        timed!("rebuilding card state", cards.apply());
        info!("state rebuilt!");
        return;
    }

    info!("starting speki");

    dioxus::launch(TheApp);
}

fn handle_add_card(args: &Vec<String>) {
    if !args.len() == 2 {
        std::process::exit(1);
    }

    let front = &args[0];
    let back = &args[1];

    let card_id = CardId::new_v4();
    let event = CardEvent::new_modify(
        card_id,
        CardAction::NormalType {
            front: TextData::from_raw(front),
            back: BackSide::Text(TextData::from_raw(back)),
        },
    );

    match APP.read().modify_card(event) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("{:?}", e);
            std::process::exit(1);
        }
    }

    if APP.read().load_set(Set::CLI_CARDS).is_none() {
        let action = SetAction::SetName("CLI imports".to_string());
        let event = SetEvent::new_modify(Set::CLI_CARDS, action);
        APP.read().modify_set(event).unwrap();
    }

    let event = SetEvent::new_modify(Set::CLI_CARDS, SetAction::AddInput(Input::Card(card_id)));
    APP.read().modify_set(event).unwrap();
    println!("{}", card_id);
    std::process::exit(0);
}

#[component]
pub fn TheApp() -> Element {
    use_context_provider(ReviewPage::new);
    use_context_provider(RemoteUpdate::new);

    let cli = Cli::parse();
    let config = dbg!(Config::load());

    if cli.find_duplicates {
        let duplicates = APP.read().duplicates();
        if duplicates.is_empty() {
            info!("no duplicates!");
        } else {
            info!("duplicates:");
            for card in duplicates {
                println!("{}", card);
            }
        }

        std::process::exit(0);
    }

    if cli.generate_config {
        if Config::path().exists() {
            eprintln!("error: config already exists.");
            std::process::exit(1);
        } else {
            Config::load().save_to_disk();
            std::process::exit(0);
        }
    }

    if let Some(card) = cli.maturity {
        let card = APP.read().load(card).unwrap();
        println!("maturity ml");
        speki_core::expected_gain(card.clone(), &RecallChoice::Trained.get_instance());

        println!("maturity fsrs");
        speki_core::expected_gain(card.clone(), &RecallChoice::FSRS.get_instance());

        println!("maturity avg");
        speki_core::expected_gain(card.clone(), &RecallChoice::Average.get_instance());
        println!("maturity simple");
        speki_core::expected_gain(card.clone(), &RecallChoice::Simple.get_instance());
        std::process::exit(0);
    }

    if let Some(card) = cli.plot {
        let card = APP.read().load(card).unwrap();
        speki_core::plot_the_recall(card);
        std::process::exit(0);
    }

    if cli.analyze {
        let histories = APP.read().load_all_histories();

        dbg!(histories.len());

        let mut training_data: Vec<History> = vec![];
        let mut eval_data: Vec<History> = vec![];
        let mut all_data: Vec<History> = vec![];

        for history in histories {
            let mut history = history.clone_item();
            if !APP.read().card_exists(history.id) {
                continue;
            };

            history.reviews.dedup();

            all_data.push(history.clone());

            if history.id.as_u128() % 100 < 80 {
                training_data.push(history);
            } else {
                eval_data.push(history);
            }
        }

        let trained = Trained::new(&training_data);

        //let eval_data = all_data;

        println!("starting default analyze algo");
        let res = log_loss_accuracy(&eval_data, SimpleRecall);
        println!("old log loss error: {res}");

        println!("starting ML algo");
        let res = log_loss_accuracy(&eval_data, trained.clone());
        println!("trained log loss error: {res}");

        let res = log_loss_accuracy(&eval_data, Trained::from_static());
        println!("cached log loss error: {res}");

        let res = log_loss_accuracy(&eval_data, FSRS);
        println!("fsrs log loss error: {res}");

        let res = log_loss_accuracy(&eval_data, AvgRecall::default());
        println!("avg log loss error: {res}");

        for alpha in 1..11 {
            let alpha = alpha as f32 / 10.;
            dbg!(alpha);
            let avg = AvgRecall {
                trained: Trained::from_static(),
                simple: FSRS,
                alpha,
            };
            let res = log_loss_accuracy(&eval_data, avg);
            println!("averager log loss error: {res}");
        }

        std::process::exit(0);
    }

    if let Some(args) = cli.add {
        handle_add_card(&args);
    }

    if let Some(id) = cli.view_front {
        let front = APP
            .read()
            .load(id)
            .map(|c| c.front_side().to_string())
            .unwrap_or(format!("<card not found>"));
        println!("{front}");
        std::process::exit(0);
    }

    if let Some(id) = cli.view_back {
        let back = APP
            .read()
            .load(id)
            .map(|c| c.display_backside().to_string())
            .unwrap_or(format!("<card not found>"));
        println!("{back}");
        std::process::exit(0);
    }

    if let Some(grade) = cli.grade {
        let recall: Recall = match grade.to_string().parse() {
            Ok(recall) => recall,
            Err(_) => {
                panic!("invalid recall");
            }
        };

        let card = match cli.card {
            Some(card) => card,
            None => panic!("card must be specified for review"),
        };

        let current_time = APP.read().current_time();
        let review = TheReview {
            timestamp: current_time,
            grade: recall,
        };

        APP.read()
            .modify_history(ReviewEvent::new_modify(card, ReviewAction::Insert(review)))
            .unwrap();

        std::process::exit(0);
    }

    if let Some(path) = cli.import_cards {
        let mut events: Vec<CardEvent> = vec![];
        let mut paths: Vec<PathBuf> = vec![];

        for path in std::fs::read_dir(&path).unwrap() {
            paths.push(path.unwrap().path());
        }

        paths.sort();

        for path in paths {
            let s = fs::read_to_string(&path).unwrap();
            let event: CardEvent = serde_json::from_str(&s).unwrap();
            events.push(event);
        }

        let qty = events.len();

        for event in events {
            APP.read().modify_card(event).unwrap();
        }

        println!("ran {} events", qty);
        std::process::exit(0);
    }

    if let Some(commit) = cli.commit {
        let upstream_url = format!(
            "https://github.com/{}/{}",
            config.remote_github_username, config.remote_github_repo
        );
        APP.read()
            .modify_card(ledgerstore::LedgerEvent::SetUpstream {
                commit,
                upstream_url,
            })
            .unwrap();
    }

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
}

impl Route {
    pub fn label(&self) -> &'static str {
        match self {
            Route::Review {} => "review",
            Route::Add {} => "add cards",
            Route::Browse {} => "browse",
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

    pub const GRAY_BUTTON: &str = "\
mt-2 inline-flex items-center text-white \
bg-gray-600 border-0 py-1 px-3 focus:outline-none hover:bg-gray-500 \
disabled:bg-gray-300 disabled:cursor-not-allowed disabled:opacity-50 \
rounded text-base md:mt-0";

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
