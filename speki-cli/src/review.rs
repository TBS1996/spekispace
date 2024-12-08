use crate::{
    add_any_card, get_timestamp, new_attribute, new_class, opt_input, print_card_info,
    utils::{
        clear_terminal, edit_with_vim, get_input, notify, select_from_all_cards,
        select_from_all_class_cards, select_from_all_instance_cards, select_from_attributes,
        select_from_class_attributes, select_from_subclass_cards,
    },
};
use dialoguer::{theme::ColorfulTheme, Input, Select};
use rand::prelude::*;
use speki_core::{
    AnyType, Attribute, AttributeCard, BackSide, Card, CardId, ClassCard, EventCard, InstanceCard,
    StatementCard,
};
use speki_dto::Recall;
use speki_provider::paths;
use std::{ops::ControlFlow, str::FromStr};

fn review_help() -> &'static str {
    r#"

possible commands:

1 =>        failed to recall backside, where the backside info seems new to you
2 =>        failed ot recall backside but the information was familiar to you when reading it
3 =>        successfully recalled backside after some thinking
4 =>        successfully recalled backside without hesitation
skip | s => skip card
y =>        add new dependency, from cards in your collections
t =>        add new dependent, from cards in your collections
Y =>        add new dependency by creating a new card
T =>        add new dependent, by creating a new card
edit =>     open the card in vim (must be installed)
delete =>   delete the card
exit =>     back to main menu
help | ? => open this help message
    "#
}

#[derive(Clone)]
enum CardAction {
    NewDependency,
    OldDependency,
    NewDependent,
    OldDependent,
    Edit,
    Delete,
    /// Turn card into an instance of a new class
    NewClass,
    /// Turn card into an instance of an old class
    OldClass,
    NewAttribute,
    OldAttribute,
    FillAttribute,
    SetBackRef,
    /// Set the parent class of current class
    ParentClass,

    NewCard,
    /// Turn card into statement
    IntoStatement,
    /// Turn a card into an attribute
    IntoAttribute,
    /// Turn card into a class
    IntoClass,

    IntoInstance,
    IntoEvent,
}

#[derive(Clone)]
enum ReviewAction {
    Grade(Recall),
    Help,
    Skip,
}

impl FromStr for CardAction {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim() {
            "y" => Self::OldDependency,
            "Y" => Self::NewDependency,
            "t" => Self::OldDependent,
            "T" => Self::NewDependent,
            "c" => Self::OldClass,
            "C" => Self::NewClass,
            "p" => Self::ParentClass,
            "a" => Self::OldAttribute,
            "A" => Self::NewAttribute,
            "fa" => Self::FillAttribute,
            "ref" => Self::SetBackRef,
            "n" => Self::NewCard,
            "edit" => Self::Edit,
            "delete" => Self::Delete,
            "ic" => Self::IntoClass,
            "ia" => Self::IntoAttribute,
            "is" => Self::IntoStatement,
            "ii" => Self::IntoInstance,
            "ie" => Self::IntoEvent,
            _ => return Err(()),
        })
    }
}

impl FromStr for ReviewAction {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim() {
            "1" => Self::Grade(Recall::None),
            "2" => Self::Grade(Recall::Late),
            "3" => Self::Grade(Recall::Some),
            "4" => Self::Grade(Recall::Perfect),
            "help" | "?" => Self::Help,
            "skip" | "s" => Self::Skip,
            _ => return Err(()),
        })
    }
}

use speki_core::App;

pub async fn review_menu(app: &App) {
    let items = vec!["Old cards", "Pending cards", "exit"];

    let selection = Select::with_theme(&ColorfulTheme::default())
        .items(&items)
        .default(0)
        .interact()
        .unwrap();

    match selection {
        0 => review_old(app).await,
        1 => review_new(app).await,
        2 => return,
        _ => panic!(),
    }
}

const DEFAULT_FILTER: &'static str =
    "recall < 0.8 & finished == true & suspended == false & resolved == true & minrecrecall > 0.8 & minrecstab > 10 & lastreview > 0.5 & weeklapses < 3 & monthlapses < 6";

pub async fn review_new(app: &App) {
    let filter = DEFAULT_FILTER.to_string();
    let mut cards = app.load_pending(Some(filter)).await;
    cards.shuffle(&mut thread_rng());

    review(app, cards).await;
}

pub async fn review_old(app: &App) {
    let filter = DEFAULT_FILTER.to_string();
    let mut cards = app.load_non_pending(Some(filter)).await;
    cards.shuffle(&mut thread_rng());

    review(app, cards).await;
}

async fn handle_review_action(app: &App, card: CardId, action: ReviewAction) -> ControlFlow<()> {
    let mut card = app.load_card(card).await.unwrap();
    match action {
        ReviewAction::Grade(grade) => {
            card.add_review(grade).await;
            ControlFlow::Break(())
        }
        ReviewAction::Skip => ControlFlow::Break(()),
        ReviewAction::Help => {
            notify(format!("{}", review_help()));
            ControlFlow::Continue(())
        }
    }
}

async fn create_attribute_card(card: &Card<AnyType>, app: &App) -> Option<AttributeCard> {
    notify(format!("Which instance ?"));
    let instance_id = select_from_all_instance_cards(app).await?;
    let instance = app.load_card(instance_id).await.unwrap();

    notify(format!("Which attribute among the class?"));
    let attribute_id = select_from_class_attributes(app, instance.parent_class().unwrap()).await?;

    let attribute = app.load_attribute(attribute_id).await.unwrap();

    let back = if let Some(back_type) = attribute.back_type {
        let class_name = app.load_card(back_type).await.unwrap().print().await;
        notify(format!(
            "chosen attribute requires card belonging to this class: {}",
            class_name,
        ));

        let back = select_from_subclass_cards(app, back_type).await?;

        BackSide::Card(back)
    } else {
        match card.back_side() {
            Some(back) => back.clone(),
            None => {
                let answer: String = Input::new()
                    .with_prompt("answer to question: ")
                    .allow_empty(true)
                    .interact_text()
                    .expect("Failed to read input");
                if answer.is_empty() {
                    return None;
                }

                BackSide::Text(answer)
            }
        }
    };

    Some(AttributeCard {
        attribute: attribute.id,
        back,
        instance: instance_id,
        card_provider: app.card_provider.clone(),
    })
}

async fn handle_action(app: &App, card: CardId, action: CardAction) -> ControlFlow<()> {
    let card = app.load_card(card).await.unwrap();

    match action {
        CardAction::IntoAttribute => match card.card_type() {
            AnyType::Normal(_) | AnyType::Unfinished(_) => {
                if let Some(attr) = create_attribute_card(&card, app).await {
                    card.into_type(attr).await;
                }
            }
            AnyType::Attribute(_) => {}
            AnyType::Instance(_) => {}
            AnyType::Class(_) => {}
            AnyType::Statement(_) => {}
            AnyType::Event(_) => {}
        },

        CardAction::IntoInstance => {
            if let Some(class) = select_from_all_class_cards(app).await {
                let instance = InstanceCard {
                    name: card.print().await,
                    class,
                    back: card.back_side().map(ToOwned::to_owned),
                };

                card.into_type(instance).await;
            }
        }

        CardAction::NewDependency => {
            println!("add dependency");
            if let Some(new_card) = add_any_card(app).await {
                app.load_card(card.id)
                    .await
                    .unwrap()
                    .add_dependency(new_card)
                    .await;
            }
        }
        CardAction::OldDependency => {
            if let Some(dep) = select_from_all_cards(app).await {
                app.load_card(card.id())
                    .await
                    .unwrap()
                    .add_dependency(dep)
                    .await;
            }
        }
        CardAction::NewDependent => {
            println!("add dependent");
            if let Some(new_card) = add_any_card(app).await {
                app.load_card(new_card)
                    .await
                    .unwrap()
                    .add_dependency(card.id)
                    .await;
            }
        }
        CardAction::OldDependent => {
            if let Some(dep) = select_from_all_cards(app).await {
                app.load_card(dep)
                    .await
                    .unwrap()
                    .add_dependency(card.id)
                    .await;
            }
        }
        CardAction::OldClass => {
            if let Some(concept) = select_from_all_class_cards(app).await {
                app.set_class(card.id(), concept).await.unwrap();
            }
        }
        CardAction::NewClass => {
            if let Some(class) = new_class() {
                let class = app.new_any(class).await;
                app.set_class(card.id(), class.id()).await.unwrap();
            }
        }
        CardAction::FillAttribute => {
            if card.is_instance() {
                let attributes = Attribute::load_relevant_attributes(app, card.id()).await;

                if let Some(attribute) = select_from_attributes(attributes) {
                    let attr = app.load_attribute(attribute).await.unwrap();
                    let txt = attr.name(card.id()).await;

                    if let Some(back) = opt_input(&txt) {
                        let attr = AttributeCard {
                            attribute,
                            back: back.into(),
                            instance: card.id(),
                            card_provider: app.card_provider.clone(),
                        };

                        app.new_any(attr).await;
                    }
                }
            }
        }

        // Marks this card as an attribute
        CardAction::OldAttribute => {
            if let Some(attr) = new_attribute(app).await {
                card.into_type(attr).await;
            }
        }
        CardAction::NewAttribute => {
            if let Some(class) = card.parent_class().or(card.is_class().then_some(card.id())) {
                if let Some(pattern) = opt_input("attribute pattern") {
                    notify("which class should the answer belong to?");
                    let back_type = select_from_all_class_cards(app).await;
                    Attribute::create(app, pattern, class, back_type).await;
                    notify("new pattern created");
                }
            } else {
                notify("current card must belong to a class");
            }
        }

        CardAction::IntoClass => {
            let front = card.print().await;
            let back = card.back_side().map(ToOwned::to_owned).unwrap_or_default();
            let class = ClassCard {
                name: front,
                back,
                parent_class: None,
            };

            card.into_type(class).await;
        }

        CardAction::IntoStatement => {
            let statement = StatementCard {
                front: card.print().await,
            };

            card.into_type(statement).await;
        }

        CardAction::IntoEvent => {
            let event = EventCard {
                front: card.print().await,
                start_time: get_timestamp(&card.print().await),
                end_time: None,
                parent_event: None,
            };

            card.into_type(event).await;
        }

        CardAction::ParentClass => {
            if let AnyType::Class(class) = card.card_type() {
                if let Some(parent_class) = select_from_all_class_cards(app).await {
                    if parent_class != card.id() {
                        let mut class = class.clone();
                        class.parent_class = Some(parent_class);
                        card.into_type(class).await;
                    }
                }
            } else {
                notify("parent class can only be set for class");
            }
        }

        CardAction::SetBackRef => {
            if let Some(reff) = select_from_all_cards(app).await {
                app.load_card(card.id()).await.unwrap().set_ref(reff).await;
            }
        }
        CardAction::Edit => {
            let _ = edit_with_vim(card.id());
        }
        CardAction::Delete => {
            app.delete_card(card.id()).await;
            return ControlFlow::Break(());
        }

        CardAction::NewCard => {
            let _ = add_any_card(app);
        }
    }

    ControlFlow::Continue(())
}

pub async fn view_card(app: &App, mut card: CardId, mut review_mode: bool) -> ControlFlow<()> {
    let mut show_backside = !review_mode;

    loop {
        if print_card(app, card, show_backside).await.is_break() {
            return ControlFlow::Continue(());
        }

        show_backside = true;

        let txt: String = get_input("");

        if let Ok(action) = txt.parse::<ReviewAction>() {
            if review_mode {
                match handle_review_action(app, card, action).await {
                    ControlFlow::Continue(_) => continue,
                    ControlFlow::Break(_) => return ControlFlow::Continue(()),
                }
            }
        }

        if let Ok(action) = txt.parse::<CardAction>() {
            match handle_action(app, card, action).await {
                ControlFlow::Continue(_) => continue,
                ControlFlow::Break(_) => return ControlFlow::Continue(()),
            }
        } else {
            if txt.contains("exit") {
                return ControlFlow::Break(());
            } else if txt.contains("revs") {
                let path = paths::get_review_path().join(card.to_string());
                opener::open(path).unwrap();
            }

            if txt.contains("find") {
                if let Some(newcard) = select_from_all_cards(app).await {
                    card = newcard;
                    review_mode = false;
                }

                continue;
            }

            clear_terminal();

            Select::with_theme(&ColorfulTheme::default())
                .with_prompt("write 'help' to see list of possible action")
                .items(&["back to card"])
                .default(0)
                .interact()
                .expect("Failed to make selection");
        };
    }
}

async fn print_card(app: &App, card: CardId, mut show_backside: bool) -> ControlFlow<()> {
    clear_terminal();
    let card = app.load_card(card).await.unwrap();

    let var_name = match card.card_type() {
        AnyType::Instance(instance) => match card.back_side() {
            Some(_) => {
                let parent_class = app.load_card(instance.class).await.unwrap();
                let front = format!(
                    "what is: {} ({})",
                    card.print().await,
                    parent_class.print().await
                );
                let back = card.display_backside().await.unwrap_or_default();
                (front, back)
            }
            None => {
                let front = format!("which class: {}", card.print().await);
                let back = app.load_card(instance.class).await.unwrap().print().await;
                (front, back)
            }
        },

        AnyType::Normal(_) => {
            let front = card.print().await;
            let back = card.display_backside().await.unwrap_or_default();
            (front, back)
        }
        AnyType::Unfinished(_) => {
            show_backside = true;
            let front = card.print().await;
            let back = String::from("card has no answer yet");
            (front, back)
        }
        AnyType::Attribute(_) => {
            let front = card.print().await;
            let back = card.display_backside().await.unwrap_or_default();
            (front, back)
        }
        AnyType::Class(_) => {
            let front = card.print().await;
            let back = card.display_backside().await.unwrap_or_default();
            (front, back)
        }
        AnyType::Statement(_) | AnyType::Event(_) => {
            show_backside = true;
            let front = card.print().await;
            let back = String::default();
            (front, back)
        }
    };
    let (front, back) = var_name;

    let opts = ["reveal answer"];

    println!(
        "recall: {:.1}%, stability: {:.2} days, card_type: {}",
        (card.recall_rate().unwrap_or_default() * 100.),
        card.maturity(),
        card.card_type().type_name()
    );
    println!();
    println!("{}", &front);
    if !show_backside {
        println!();
        match Select::with_theme(&ColorfulTheme::default())
            .with_prompt("")
            .items(&opts)
            .default(0)
            .interact()
            .expect("Failed to make selection")
        {
            0 => {
                clear_terminal();
                println!(
                    "recall: {:.1}%, stability: {:.2} days, card_type: {}",
                    (card.recall_rate().unwrap_or_default() * 100.),
                    card.maturity(),
                    card.card_type().type_name()
                );
                println!();
                println!("{}", &front);
                println!();
                println!("-------------------------------------------------");
                println!();
            }
            _ => return ControlFlow::Break(()),
        }
    }

    println!("{}", &back);
    println!();
    print_card_info(app, card.id()).await;
    ControlFlow::Continue(())
}

pub async fn review(app: &App, cards: Vec<CardId>) {
    if cards.is_empty() {
        clear_terminal();
        notify("nothing to review!");
        return;
    } else {
        clear_terminal();
        notify(format!("reviewing {} cards", cards.len()));
    }

    for card in cards {
        if view_card(app, card, true).await.is_break() {
            return;
        }
    }
}
