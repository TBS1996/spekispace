#![allow(non_snake_case)]

use std::{
    path::PathBuf,
    str::FromStr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use clap::{Args, Parser, ValueEnum};
use dioxus::prelude::*;
use dioxus_logger::tracing::{info, Level};
use indexmap::IndexSet;
use ledgerstore::{ItemAction, ItemExpr, Ledger};
use pages::ReviewPage;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use speki_core::{
    card::{
        bigrams_expression_and, AttrBackType, Attrv2, BackSide, CType, CardId, RawCard, TextData,
    },
    ledger::{CardAction, CardEvent},
    log_loss_accuracy,
    recall_rate::{
        ml::classic::Trained, AvgRecall, History, Recall, Review as TheReview, ReviewAction,
        ReviewEvent, FSRS,
    },
    set::{Input, Set, SetAction, SetEvent, SetId},
    CardProperty, Config, RecallChoice, SimpleRecall,
};
use std::fs;
use uuid::Uuid;

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
    view: Option<CardId>,
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
    check_into_events: bool,
    #[arg(long)]
    rebuild_state: bool,
    #[arg(long)]
    load_cards: bool,
    #[arg(long)]
    set: Option<String>,

    /// Create or modify a card by passing a JSON CardAction. Can be combined with --card to modify an existing card.
    /// Example: --action '{"NormalType":{"front":{"Raw":"What is Rust?"},"back":{"Text":{"Raw":"A systems programming language"}}}}'
    #[arg(long)]
    action: Option<String>,

    /// Add an attribute to a class card. Requires --card to specify the class.
    /// Format: --add-attribute '{"pattern":"When was this person born?","back_type":{"Time":null}}'
    #[arg(long)]
    add_attribute: Option<String>,

    /// Add a parameter to a class card. Requires --card to specify the class.
    /// Format: --add-parameter '{"pattern":"Which crate?","back_type":{"Text":null}}'
    #[arg(long)]
    add_parameter: Option<String>,

    /// Show class information including all inherited attributes from parent classes.
    /// Requires --card to specify the class.
    #[arg(long)]
    show_class_info: bool,

    /// Get id of card that exactly matches the given string.
    #[arg(long)]
    exact_match: Option<String>,

    #[command(flatten)]
    load_card_args: LoadCardsArgs,
}

#[derive(Args, Debug, Default)]
struct LoadCardsArgs {
    #[command(flatten)]
    filter: CardFilters,

    #[arg(long)]
    format: Option<OutputFormat>,

    #[arg(long)]
    limit: Option<i32>,
}

#[derive(Args, Debug, Default)]
struct CardFilters {
    #[arg(long)]
    contains: Option<String>,
    #[arg(long)]
    card_type: Option<String>,
    #[arg(long)]
    trivial: Option<bool>,
}

impl CardFilters {
    fn as_expression(&self) -> ItemExpr<RawCard> {
        let mut exprs: Vec<ItemExpr<RawCard>> = vec![];

        if let Some(ctype) = self.card_type.as_ref() {
            let ctype = CType::from_str(&ctype).unwrap().to_string();

            exprs.push(ItemExpr::Property {
                property: CardProperty::CardType,
                value: ctype,
            });
        }

        if let Some(ref search_str) = self.contains {
            exprs.push(bigrams_expression_and(&search_str));
        }

        if let Some(trivial) = self.trivial {
            exprs.push(ItemExpr::Property {
                property: CardProperty::Trivial,
                value: trivial.to_string(),
            });
        }

        if exprs.is_empty() {
            ItemExpr::All
        } else {
            ItemExpr::Intersection(exprs)
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
    Id,
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
    unsafe {
        std::env::set_var("GDK_BACKEND", "x11");
    }

    let cli = Cli::parse();

    let mut log_level = if cli.trace { Level::DEBUG } else { Level::INFO };

    let headless = cli.add.is_some()
        || cli.view_back.is_some()
        || cli.view_front.is_some()
        || cli.commit.is_some()
        || cli.import_cards.is_some()
        || cli.grade.is_some()
        || cli.load_cards
        || cli.action.is_some()
        || cli.set.is_some()
        || cli.exact_match.is_some();

    if headless {
        log_level = Level::ERROR;
    }

    dioxus_logger::init(log_level).expect("failed to init logger");

    if let Some(card) = cli.view {
        let path = Config::load().storage_path.clone();
        let app = speki_core::App::new(path);
        let raw = app.card_provider.providers.cards.load(card).unwrap();
        let raw = Arc::unwrap_or_clone(raw);

        serde_json::to_writer_pretty(std::io::stdout(), &raw).unwrap();
        return;
    }

    if cli.load_cards {
        handle_load_cards(&cli.load_card_args);
        return;
    } else if cli.review {
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
    } else if cli.check_into_events {
        let path = Config::load().storage_path.clone();
        let app = speki_core::App::new(path);

        for (idx, card) in app
            .card_provider
            .providers
            .cards
            .load_all()
            .into_iter()
            .enumerate()
        {
            if idx % 100 == 0 {
                info!("checking card {}", idx);
            }
            match card.check_into_events() {
                Ok(()) => {}
                Err((old, new)) => {
                    dbg!(old, new);
                }
            }
        }

        println!("all cards checked into events");

        return;
    } else if cli.card.is_some() && cli.set.is_some() && cli.action.is_none() {
        let path = Config::load().storage_path.clone();
        let app = speki_core::App::new(path);
        let set: SetId = match cli.set.clone().unwrap().parse() {
            Ok(set) => set,
            Err(_) => {
                let set = cli.set.clone().unwrap();
                let id = uuid_from_hash(&set);
                if app.provider.sets.load(id).is_none() {
                    let action = SetAction::SetName(set);
                    app.provider.sets.modify_action(id, action).unwrap();
                }
                id
            }
        };
        let card = cli.card.unwrap();

        let res = app
            .provider
            .sets
            .modify_action(set, SetAction::AddInput(Input::Card(card)));

        println!("{:?}", res);
        return;
    } else if let Some(s) = cli.exact_match {
        let path = Config::load().storage_path.clone();
        let app = speki_core::App::new(path);

        match app.card_provider.exact_match(&s) {
            Some(card_id) => {
                println!("{}", card_id);
                std::process::exit(0);
            }
            None => {
                eprintln!("No exact match found for: {}", s);
                std::process::exit(1);
            }
        }
    }

    info!("starting speki");

    dioxus::launch(TheApp);
}

fn print_card_with_format(card: &speki_core::card::Card, format: OutputFormat) -> String {
    match format {
        OutputFormat::Text => {
            let mut s = format!("Q: {}", card.front_side().to_string());
            if card.back_side().is_some() {
                s.push_str(&format!(". A: {}", card.backside().to_string()));
            }
            s
        }
        OutputFormat::Id => card.id().to_string(),
        OutputFormat::Json => json!({
            "id": card.id().to_string(),
            "front": card.name().to_string(),
            "back": card.back_side().as_ref().map(|x| x.to_string())
        })
        .to_string(),
    }
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

fn handle_card_action(action_json: &str, card_id: Option<CardId>, set: Option<String>) {
    // Parse the JSON into a CardAction
    let action: CardAction = match serde_json::from_str(action_json) {
        Ok(action) => action,
        Err(e) => {
            eprintln!("Error parsing CardAction JSON: {}", e);
            eprintln!("Hint: Make sure your JSON matches the CardAction enum structure.");
            std::process::exit(1);
        }
    };

    // Reject actions that should use dedicated commands
    match &action {
        CardAction::InsertAttr(_) | CardAction::SetAttrs(_) => {
            eprintln!("Error: InsertAttr and SetAttrs cannot be used directly via --action.");
            eprintln!("Use --add-attribute instead to create attributes with auto-generated IDs.");
            eprintln!("Example: --card <class-id> --add-attribute '{{\"pattern\":\"When was this born?\",\"back_type\":{{\"Time\":null}}}}'.");
            std::process::exit(1);
        }
        CardAction::SetParams(_) | CardAction::InsertParam(_) => {
            eprintln!("Error: SetParams and InsertParam cannot be used directly via --action.");
            eprintln!("Use --add-parameter instead to create parameters with auto-generated IDs.");
            eprintln!("Example: --card <class-id> --add-parameter '{{\"pattern\":\"Which crate?\",\"back_type\":{{\"Text\":null}}}}'.");
            std::process::exit(1);
        }
        _ => {}
    }

    // Determine if we're creating a new card
    let is_new_card = card_id.is_none();
    let target_card_id = card_id.unwrap_or_else(CardId::new_v4);

    // Create the ItemAction
    let item_action = ItemAction::new_modify(target_card_id, action);

    // Apply the action
    match APP.read().0.apply_action(item_action) {
        Ok(change) => {
            // If this was a new card, add it to the appropriate set
            if is_new_card {
                let set_id = if let Some(set_name) = set.clone() {
                    // Generate deterministic set ID from name
                    uuid_from_hash(&set_name)
                } else {
                    // Default to CLI_CARDS set
                    Set::CLI_CARDS
                };

                // Ensure the set exists
                if APP.read().load_set(set_id).is_none() {
                    let set_name_str = set.unwrap_or_else(|| "CLI imports".to_string());
                    let action = SetAction::SetName(set_name_str);
                    let event = SetEvent::new_modify(set_id, action);
                    APP.read().modify_set(event).unwrap();
                }

                // Add card to the set
                let event =
                    SetEvent::new_modify(set_id, SetAction::AddInput(Input::Card(target_card_id)));
                APP.read().modify_set(event).unwrap();
            }

            println!("{}", change.print_terse());
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("Error modifying card: {:?}", e);
            std::process::exit(1);
        }
    }
}

#[derive(Deserialize)]
struct AttributeInput {
    pattern: String,
    back_type: Option<AttrBackType>,
}

fn handle_add_attribute(attr_json: &str, class_id: CardId) {
    // Parse the JSON into AttributeInput
    let input: AttributeInput = match serde_json::from_str(attr_json) {
        Ok(input) => input,
        Err(e) => {
            eprintln!("Error parsing attribute JSON: {}", e);
            eprintln!("Expected format: '{{\"pattern\":\"question text\",\"back_type\":{{\"Time\":null}}}}'");
            std::process::exit(1);
        }
    };

    // Generate a new AttributeId
    let attr_id = uuid::Uuid::new_v4();

    // Create the Attrv2
    let attr = Attrv2 {
        id: attr_id,
        pattern: input.pattern,
        back_type: input.back_type,
    };

    // Create the action
    let action = CardAction::InsertAttr(attr);
    let item_action = ItemAction::new_modify(class_id, action);

    // Apply the action
    match APP.read().0.apply_action(item_action) {
        Ok(_) => {
            println!("{}", attr_id);
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("Error adding attribute: {:?}", e);
            std::process::exit(1);
        }
    }
}

fn handle_add_parameter(param_json: &str, class_id: CardId) {
    // Parse the JSON into AttributeInput (parameters use the same structure)
    let input: AttributeInput = match serde_json::from_str(param_json) {
        Ok(input) => input,
        Err(e) => {
            eprintln!("Error parsing parameter JSON: {}", e);
            eprintln!("Expected format: '{{\"pattern\":\"question text\",\"back_type\":{{\"Text\":null}}}}'");
            std::process::exit(1);
        }
    };

    // Generate a new parameter ID
    let param_id = uuid::Uuid::new_v4();

    // Create the Attrv2 (parameters use the same struct)
    let param = Attrv2 {
        id: param_id,
        pattern: input.pattern,
        back_type: input.back_type,
    };

    // Create the action
    let action = CardAction::InsertParam(param);
    let item_action = ItemAction::new_modify(class_id, action);

    // Apply the action
    match APP.read().0.apply_action(item_action) {
        Ok(_) => {
            println!("{}", param_id);
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("Error adding parameter: {:?}", e);
            std::process::exit(1);
        }
    }
}

fn handle_show_class_info(class_id: CardId) {
    let path = Config::load().storage_path.clone();
    let app = speki_core::App::new(path);

    let Some(card) = app.card_provider.load(class_id) else {
        eprintln!("Card not found: {}", class_id);
        std::process::exit(1);
    };

    if !card.is_class() {
        eprintln!("Error: Card {} is not a class", class_id);
        std::process::exit(1);
    }

    let direct_attrs = card.attributes_on_class().unwrap();

    // Get all attributes including inherited ones
    let all_attrs = card.attributes().unwrap();

    // Get parent class
    let parent = card.parent_class();

    // Build output
    let mut output = serde_json::json!({
        "id": class_id,
        "name": card.front_side().to_string(),
        "parent_class": parent,
    });

    // Add attributes info
    let mut attrs_info = vec![];
    for attr in &all_attrs {
        let is_direct = direct_attrs.contains(attr);
        attrs_info.push(serde_json::json!({
            "id": attr.id,
            "pattern": attr.pattern,
            "back_type": attr.back_type,
            "inherited": !is_direct,
        }));
    }

    output["attributes"] = serde_json::json!(attrs_info);

    serde_json::to_writer_pretty(std::io::stdout(), &output).unwrap();
    println!(); // Add newline at end
    std::process::exit(0);
}

fn handle_load_cards(
    LoadCardsArgs {
        filter,
        format,
        limit,
    }: &LoadCardsArgs,
) {
    let path = Config::load().storage_path.clone();
    let app = speki_core::App::new(path);

    let limit = limit
        .map(|limit| {
            if limit == 0 {
                usize::MAX
            } else {
                limit as usize
            }
        })
        .unwrap_or(10);

    let format = format.unwrap_or_default();

    let cards = if filter.contains.is_some() {
        let other_filters = CardFilters {
            contains: None,
            card_type: filter.card_type.clone(),
            trivial: filter.trivial,
        };
        let base_expr = other_filters.as_expression();
        let candidate_cards: IndexSet<speki_core::card::CardId> = app
            .card_provider
            .providers
            .cards
            .load_expr(base_expr)
            .into_iter()
            .collect();

        let search_text = filter.contains.as_ref().unwrap();
        let normalized_search = speki_core::card::normalize_string(search_text);
        let search_results = speki_core::card::search_cards_by_text(
            &normalized_search,
            &candidate_cards,
            &app.card_provider.providers.cards,
            limit,
        );

        search_results.into_iter().map(|(_, id)| id).collect()
    } else {
        let expr = filter.as_expression();
        app.card_provider.providers.cards.load_expr(expr)
    };

    let qty = cards.len();
    let overflow = if qty > limit { qty - limit } else { 0 };

    for (idx, card) in cards.into_iter().enumerate() {
        if idx >= limit {
            break;
        }

        let card = app.load_card(card).unwrap();
        print!("{}", print_card_with_format(&card, format));

        if idx < qty - 1 {
            println!();
        }
    }

    if overflow > 0 {
        eprintln!(
            "output truncated, {} more cards. use --limit to increase the limit. 0 to remove limit",
            overflow
        );
    }
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

        for mut history in histories {
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

    if let Some(action_json) = cli.action {
        handle_card_action(&action_json, cli.card, cli.set);
    }

    if let Some(attr_json) = cli.add_attribute {
        let class_id = cli
            .card
            .expect("--add-attribute requires --card to specify the class");
        handle_add_attribute(&attr_json, class_id);
    }

    if let Some(param_json) = cli.add_parameter {
        let class_id = cli
            .card
            .expect("--add-parameter requires --card to specify the class");
        handle_add_parameter(&param_json, class_id);
    }

    if cli.show_class_info {
        let class_id = cli
            .card
            .expect("--show-class-info requires --card to specify the class");
        handle_show_class_info(class_id);
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

/// Generate a UUID from the SHA-256 hash of the input data
///
/// Kinda hacky, since uuids are supposed to be random.
pub fn uuid_from_hash(input: impl AsRef<[u8]>) -> Uuid {
    let hash = Sha256::digest(input.as_ref());

    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hash[..16]);

    // RFC 4122 variant
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    // Version 4 layout
    bytes[6] = (bytes[6] & 0x0f) | 0x40;

    Uuid::from_bytes(bytes)
}
