use add_cards::add_cards_menu;
use clap::Parser;
use console::style;
use dialoguer::{theme::ColorfulTheme, Input, Select};
use incread::inc_path;
use review::{review_menu, view_card};
use speki_core::{
    attribute::Attribute,
    card::{
        AnyType, AttributeCard, ClassCard, EventCard, InstanceCard, NormalCard, StatementCard,
        UnfinishedCard,
    },
    // github::{poll_for_token, request_device_code, LoginInfo},
    BackSide,
    CType,
    Card,
    CardId,
    TimeStamp,
};
use utils::{
    notify, select_from_all_cards, select_from_all_class_cards, select_from_all_instance_cards,
    select_from_attributes,
};

use speki_fs::paths::{config_dir, get_cards_path, get_review_path};

//mod collections;
mod add_cards;
mod incread;
mod review;
mod unfinished;
mod utils;

pub fn opt_input(prompt: &str) -> Option<String> {
    let input: String = Input::new()
        .with_prompt(prompt)
        .allow_empty(true)
        .interact_text()
        .expect("Failed to read input");

    if input.is_empty() {
        None
    } else {
        Some(input)
    }
}

fn new_class() -> Option<ClassCard> {
    let name = opt_input("class name")?;
    let back: BackSide = opt_input("backside")
        .map(BackSide::from)
        .unwrap_or(BackSide::Trivial);

    Some(ClassCard {
        name,
        back,
        parent_class: None,
    })
}

fn new_instance() -> Option<InstanceCard> {
    let class = select_from_all_class_cards()?;
    let name = opt_input("name of instance")?;
    Some(InstanceCard {
        name,
        class,
        back: None,
    })
}

fn new_attribute() -> Option<AttributeCard> {
    notify("which instance card is this attribute for?");

    let instance = select_from_all_instance_cards()?;
    let attribute = {
        let attributes = Attribute::load_relevant_attributes(instance);
        if attributes.is_empty() {
            notify("no relevant attributes found for instance");
            return None;
        }
        select_from_attributes(attributes)?
    };

    let back: BackSide = {
        let prompt = Attribute::load(attribute).unwrap().name(instance);
        opt_input(&prompt)?.into()
    };

    Some(AttributeCard {
        attribute,
        back,
        instance,
    })
}

fn new_normal() -> Option<NormalCard> {
    let front = opt_input("front")?;
    let back: BackSide = opt_input("back")?.into();
    Some(NormalCard { front, back })
}

fn new_unfinished() -> Option<UnfinishedCard> {
    let front = opt_input("front")?;
    Some(UnfinishedCard { front })
}

fn new_statement() -> Option<StatementCard> {
    let front = opt_input("statement")?;
    Some(StatementCard { front })
}

fn new_event() -> Option<EventCard> {
    let front = opt_input("event")?;
    let start_time = get_timestamp(&front);
    let end_time = None;
    let parent_event = None;

    Some(EventCard {
        front,
        start_time,
        end_time,
        parent_event,
    })
}

pub fn get_timestamp(front: &str) -> TimeStamp {
    let prompt = format!("when did {} occur?", front);
    loop {
        let timestamp: String = Input::new()
            .with_prompt(&prompt)
            .allow_empty(true)
            .interact_text()
            .expect("Failed to read input");

        match TimeStamp::from_string(timestamp) {
            Some(t) => return t,
            None => {
                notify("invalid timestamp. Please follow ISO 8601 format");
            }
        }
    }
}

pub fn create_card(ty: CType) -> Option<Card<AnyType>> {
    let ty = create_type(ty)?;
    Some(Card::new_any(ty))
}

pub fn create_type(ty: CType) -> Option<AnyType> {
    let any: AnyType = match ty {
        CType::Instance => new_instance()?.into(),
        CType::Normal => new_normal()?.into(),
        CType::Unfinished => new_unfinished()?.into(),
        CType::Attribute => new_attribute()?.into(),
        CType::Class => new_class()?.into(),
        CType::Statement => new_statement()?.into(),
        CType::Event => new_event()?.into(),
    };

    Some(any)
}

pub fn choose_type() -> Option<CType> {
    let items = vec![
        "instance",
        "normal",
        "unfinished",
        "attribute",
        "class",
        "statement",
        "event",
        "cancel",
    ];

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("")
        .items(&items)
        .default(0)
        .interact()
        .unwrap();

    match selection {
        0 => CType::Instance,
        1 => CType::Normal,
        2 => CType::Unfinished,
        3 => CType::Attribute,
        4 => CType::Class,
        5 => CType::Statement,
        6 => CType::Event,
        7 => return None,
        _ => panic!(),
    }
    .into()
}

pub fn add_any_card() -> Option<CardId> {
    let ty = choose_type()?;
    Some(create_card(ty)?.id())
}

fn inspect_files() {
    let items = vec![
        "Inspect config",
        "Inspect cards",
        "Inspect reviews",
        "Inspect texts",
        "go back",
    ];

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("")
        .items(&items)
        .default(0)
        .interact()
        .unwrap();

    match selection {
        0 => opener::open(config_dir()).unwrap(),
        1 => opener::open(get_cards_path()).unwrap(),
        2 => opener::open(get_review_path()).unwrap(),
        3 => opener::open(inc_path()).unwrap(),
        4 => {}
        _ => panic!(),
    }
}

async fn menu() {
    //new_repo_col(&login.clone().unwrap(), "repo3", true);

    loop {
        utils::clear_terminal();

        let items = vec!["Review cards", "Add cards", "Inspect files", "view card"];

        let selection = Select::with_theme(&ColorfulTheme::default())
            .items(&items)
            .default(0)
            .interact()
            .unwrap();

        match selection {
            0 => review_menu(),
            1 => add_cards_menu().await,
            2 => inspect_files(),
            3 => {
                if let Some(card) = select_from_all_cards() {
                    view_card(card, false);
                }
            }
            _ => panic!(),
        }
    }
}

fn print_card_info(id: CardId) {
    let card = Card::from_id(id).unwrap();
    let dependencies = card.dependency_ids();
    let dependents = speki_core::get_cached_dependents(id);

    if let AnyType::Instance(ty) = card.card_type() {
        let concept = Card::from_id(ty.class).unwrap().print();
        println!("concept: {}", concept);
    }

    if !dependencies.is_empty() {
        println!("{}", style("dependencies").bold());
        for id in dependencies {
            println!(
                "{}",
                Card::from_id(id)
                    .map(|card| card.print())
                    .unwrap_or_else(|| format!("missing card for dependency: {id}"))
            );
        }
    }

    if !dependents.is_empty() {
        let dpt_qty = dependents.len();

        if dpt_qty > 10 {
            println!("card has {} dependents", dpt_qty);
        } else {
            println!("{}", style("dependendents").bold());
            for id in dependents {
                println!(
                    "{}",
                    Card::from_id(id)
                        .map(|card| card.print())
                        .unwrap_or_else(|| format!("missing card for dependent: {id}"))
                );
            }
        }
    }

    println!();
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    #[arg(short, long)]
    add: Option<String>,
    #[arg(short, long)]
    filter: Option<String>,
    #[arg(short, long)]
    concept: Option<String>,
    #[arg(short, long)]
    list: bool,
    #[arg(short, long)]
    graph: bool,
    #[arg(short, long)]
    prune: bool,
    #[arg(long)]
    debug: bool,
    #[arg(long)]
    recall: Option<String>,
    #[arg(long)]
    healthcheck: bool,
    #[arg(long)]
    roundtrip: bool,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if cli.add.is_some() {
        let s = cli.add.unwrap();

        if let Some((front, back)) = s.split_once(";") {
            speki_core::add_card(front.to_string(), back.to_string());
        } else {
            speki_core::add_unfinished(s);
        }
    } else if cli.list {
        dbg!(speki_core::load_cards());
    } else if cli.graph {
        println!("{}", speki_core::as_graph());
    } else if cli.prune {
        todo!()
    } else if cli.debug {
        //speki_core::fetch_repos();
        // speki_core::categories::Category::load_all();
    } else if cli.recall.is_some() {
        let id = cli.recall.unwrap();
        let id: uuid::Uuid = id.parse().unwrap();
        let id = CardId(id);
        let x = speki_core::Card::from_id(id).unwrap().recall_rate();
        dbg!(x);
    } else if cli.concept.is_some() {
    } else if cli.healthcheck {
        speki_core::health_check();
    } else if cli.roundtrip {
        speki_core::load_and_persist();
    } else {
        menu().await;
    }
}
