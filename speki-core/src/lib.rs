use attribute::Attribute;
pub use card::Card;
use card::{AnyType, AttributeCard, CardTrait, InstanceCard, NormalCard, UnfinishedCard};
use categories::Category;
use eyre::Result;
use reviews::Recall;
use samsvar::Matcher;
use sanitize_filename::sanitize;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

pub mod attribute;
pub mod card;
pub mod categories;
pub mod collections;
pub mod common;
pub mod config;
pub mod github;
pub mod paths;
pub mod recall_rate;
pub mod reviews;

pub use omtrent::TimeStamp;
pub use speki_dto::BackSide;
pub use speki_dto::CType;
pub use speki_dto::CardId;

pub fn load_cards() -> Vec<CardId> {
    Card::load_all_cards()
        .iter()
        .map(|card| card.id())
        .collect()
}

pub fn load_and_persist() {
    for mut card in Card::load_all_cards() {
        card.persist();
    }
}

pub fn get_cached_dependents(id: CardId) -> BTreeSet<CardId> {
    Card::<AnyType>::dependents(id)
}

pub fn cards_filtered(filter: String) -> Vec<CardId> {
    let mut cards = Card::load_all_cards();
    cards.retain(|card| card.clone().eval(filter.clone()));
    cards.iter().map(|card| card.id()).collect()
}

pub fn add_card(front: String, back: String, cat: &Category) -> CardId {
    let data = NormalCard {
        front,
        back: back.into(),
    };
    Card::<AnyType>::new_normal(data, cat).id()
}

pub fn add_unfinished(front: String, category: &Category) -> CardId {
    let data = UnfinishedCard { front };
    Card::<AnyType>::new_unfinished(data, category).id()
}

pub fn review(card_id: CardId, grade: Recall) {
    let mut card = Card::from_id(card_id).unwrap();
    card.new_review(grade, Default::default());
}

pub fn set_class(card_id: CardId, class: CardId) -> Result<()> {
    let card = Card::from_id(card_id).unwrap();

    let instance = InstanceCard {
        name: card.card_type().display_front(),
        back: card.back_side().map(ToOwned::to_owned),
        class,
    };
    card.into_type(instance);
    Ok(())
}

pub fn set_dependency(card_id: CardId, dependency: CardId) {
    if card_id == dependency {
        return;
    }

    let mut card = Card::from_id(card_id).unwrap();
    card.set_dependency(dependency);
    card.persist();
}

pub fn card_from_id(card_id: CardId) -> Card<AnyType> {
    Card::from_id(card_id).unwrap()
}

pub fn delete(card_id: CardId) {
    let path = Card::from_id(card_id).unwrap().as_path();
    std::fs::remove_file(path).unwrap();
}

pub fn as_graph() -> String {
    // mermaid::export()
    graphviz::export()
}

pub fn edit(card_id: CardId) {
    Card::from_id(card_id).unwrap().edit_with_vim();
}

pub fn get_containing_file_paths(directory: &Path, ext: Option<&str>) -> Vec<PathBuf> {
    let mut paths = vec![];

    for entry in std::fs::read_dir(directory).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        match ext {
            Some(ext) => {
                if path.extension().and_then(|s| s.to_str()) == Some(ext) {
                    paths.push(path)
                }
            }
            None => paths.push(path),
        }
    }
    paths
}

pub fn my_sanitize_filename(s: &str) -> String {
    sanitize(s.replace(" ", "_").replace("'", ""))
}

mod graphviz {
    use std::collections::BTreeSet;

    use super::*;

    pub fn export() -> String {
        let mut dot = String::from("digraph G {\nranksep=2.0;\nrankdir=BT;\n");
        let mut relations = BTreeSet::default();
        let cards = Card::load_all_cards();

        for card in cards {
            let label = card
                .print()
                .to_string()
                .replace(")", "")
                .replace("(", "")
                .replace("\"", "");

            let color = match card.recall_rate() {
                _ if !card.is_finished() => yellow_color(),
                Some(rate) => rate_to_color(rate as f64 * 100.),
                None => cyan_color(),
            };

            match card.recall_rate() {
                Some(rate) => {
                    let recall_rate = rate * 100.;
                    let maturity = card.maybeturity().unwrap_or_default();
                    dot.push_str(&format!(
                        "    \"{}\" [label=\"{} ({:.0}%/{:.0}d)\", style=filled, fillcolor=\"{}\"];\n",
                        card.id(),
                        label,
                        recall_rate,
                        maturity,
                        color
                    ));
                }
                None => {
                    dot.push_str(&format!(
                        "    \"{}\" [label=\"{} \", style=filled, fillcolor=\"{}\"];\n",
                        card.id(),
                        label,
                        color
                    ));
                }
            }

            // Create edges for dependencies, also enclosing IDs in quotes
            for child_id in card.dependency_ids() {
                relations.insert(format!("    \"{}\" -> \"{}\";\n", card.id(), child_id));
            }
        }

        for rel in relations {
            dot.push_str(&rel);
        }

        dot.push_str("}\n");
        dot
    }

    // Convert recall rate to a color, from red to green
    fn rate_to_color(rate: f64) -> String {
        let red = ((1.0 - rate / 100.0) * 255.0) as u8;
        let green = (rate / 100.0 * 255.0) as u8;
        format!("#{:02X}{:02X}00", red, green) // RGB color in hex
    }

    fn cyan_color() -> String {
        String::from("#00FFFF")
    }

    fn yellow_color() -> String {
        String::from("#FFFF00")
    }
}

pub fn health_check() {
    println!("STARTING HEALTH CHECK");
    verify_attributes();
    println!("HEALTH CHECK OVER");
}

fn verify_attributes() {
    for card in Card::load_all_cards() {
        if let AnyType::Attribute(AttributeCard {
            attribute,
            instance: concept_card,
            ..
        }) = card.card_type()
        {
            if Attribute::load(*attribute).is_none() {
                println!("error loading attribute for: {:?}", &card);
            }

            match Card::from_id(*concept_card) {
                Some(concept_card) => {
                    if !card.card_type().is_class() {
                        println!(
                            "error, cards concept card is not a concept: {:?} -> {:?}",
                            &card, concept_card
                        )
                    }
                }
                None => {
                    println!("error loading concept card for: {}", &card);
                }
            }
        }
    }
}
