use card::serializing::from_raw_card;
use card::RecallRate;
use eyre::Result;
use reviews::Reviews;
use samsvar::Matcher;
use speki_dto::AttributeId;
use speki_dto::Config;
use speki_dto::Recall;
use speki_dto::Review;
use speki_dto::SpekiProvider;
use std::collections::BTreeSet;
use std::fmt::Debug;
use std::time::Duration;

//pub mod collections;
//pub mod github;
mod attribute;
mod card;
mod common;
mod recall_rate;
mod reviews;

pub use attribute::Attribute;
pub use card::Card;
pub use card::{
    AnyType, AttributeCard, CardTrait, ClassCard, EventCard, InstanceCard, NormalCard,
    StatementCard, UnfinishedCard,
};
pub use common::current_time;
pub use omtrent::TimeStamp;
pub use recall_rate::SimpleRecall;
pub use speki_dto::BackSide;
pub use speki_dto::CType;
pub use speki_dto::CardId;

pub trait RecallCalc {
    fn recall_rate(&self, reviews: &Reviews, current_unix: Duration) -> Option<RecallRate>;
}

use std::sync::Arc;

impl Debug for FooBar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "provider thing")
    }
}

#[derive(Clone)]
pub struct FooBar {
    provider: Provider,
    recaller: Recaller,
}

impl FooBar {
    pub fn load_all_cards(&self) -> Vec<Card<AnyType>> {
        self.provider
            .load_all_cards()
            .into_iter()
            .map(|raw| Card::from_raw(self.clone(), raw))
            .collect()
    }

    pub fn save_card(&self, card: Card<AnyType>) {
        self.provider.save_card(from_raw_card(card));
    }

    pub fn load_card(&self, id: CardId) -> Option<Card<AnyType>> {
        self.provider
            .load_card(id)
            .map(|raw| Card::from_raw(self.clone(), raw))
    }

    pub fn delete_card(&self, id: CardId) {
        self.provider.delete_card(id);
    }

    pub fn load_all_attributes(&self) -> Vec<Attribute> {
        self.provider
            .load_all_attributes()
            .into_iter()
            .map(|dto| Attribute::from_dto(dto, self.clone()))
            .collect()
    }

    pub fn save_attribute(&self, attribute: Attribute) {
        self.provider.save_attribute(Attribute::into_dto(attribute));
    }

    pub fn load_attribute(&self, id: AttributeId) -> Option<Attribute> {
        self.provider
            .load_attribute(id)
            .map(|dto| Attribute::from_dto(dto, self.clone()))
    }

    pub fn delete_attribute(&self, id: AttributeId) {
        self.provider.delete_attribute(id);
    }

    pub fn load_reviews(&self, id: CardId) -> Reviews {
        Reviews(self.provider.load_reviews(id))
    }

    pub fn save_reviews(&self, id: CardId, reviews: Reviews) {
        self.provider.save_reviews(id, reviews.into_inner());
    }

    pub fn add_review(&self, id: CardId, review: Review) {
        self.provider.add_review(id, review);
    }

    pub fn load_config(&self) -> Config {
        Config
    }

    pub fn save_config(&self, _config: Config) {}
}

pub type Provider = Arc<Box<dyn SpekiProvider + Send>>;
pub type Recaller = Arc<Box<dyn RecallCalc + Send>>;

pub struct App {
    pub foobar: FooBar,
    pub config: Config,
}

impl Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "app!")
    }
}

impl App {
    pub fn new<A, B>(provider: A, recall_calc: B) -> Self
    where
        A: SpekiProvider + 'static + Send,
        B: RecallCalc + 'static + Send,
    {
        let config = provider.load_config();
        let foobar = FooBar {
            provider: Arc::new(Box::new(provider)),
            recaller: Arc::new(Box::new(recall_calc)),
        };

        Self { foobar, config }
    }

    pub fn load_cards(&self) -> Vec<CardId> {
        self.foobar
            .load_all_cards()
            .into_iter()
            .map(|card| card.id())
            .collect()
    }

    fn full_load_cards(&self) -> Vec<Card<AnyType>> {
        self.foobar.load_all_cards().into_iter().collect()
    }

    pub fn load_non_pending(&self, filter: Option<String>) -> Vec<CardId> {
        self.full_load_cards()
            .into_iter()
            .filter(|card| !card.history().is_empty())
            .filter(|card| {
                if let Some(ref filter) = filter {
                    card.eval(filter.clone())
                } else {
                    true
                }
            })
            .map(|card| card.id())
            .collect()
    }

    pub fn card_from_id(&self, id: CardId) -> Card<AnyType> {
        self.foobar.load_card(id).unwrap()
    }

    pub fn delete_card(&self, id: CardId) {
        self.foobar.delete_card(id);
    }

    pub fn load_and_persist(&self) {
        for mut card in self.full_load_cards() {
            card.persist();
        }
    }

    pub fn get_cached_dependents(&self, id: CardId) -> BTreeSet<CardId> {
        Card::<AnyType>::dependents(id)
    }

    pub fn cards_filtered(&self, filter: String) -> Vec<CardId> {
        let mut cards = self.full_load_cards();
        cards.retain(|card| card.eval(filter.clone()));
        cards.iter().map(|card| card.id()).collect()
    }

    pub fn add_card(&self, front: String, back: String) -> CardId {
        let data = NormalCard {
            front,
            back: back.into(),
        };
        self.new_any(data).id()
    }

    pub fn add_unfinished(&self, front: String) -> CardId {
        let data = UnfinishedCard { front };
        self.new_any(data).id()
    }

    pub fn review(&self, id: CardId, grade: Recall) {
        let review = Review {
            timestamp: current_time(),
            grade,
            time_spent: Default::default(),
        };
        self.foobar.add_review(id, review);
    }

    pub fn set_class(&self, card_id: CardId, class: CardId) -> Result<()> {
        let card = self.card_from_id(card_id);

        let instance = InstanceCard {
            name: card.card_type().display_front(),
            back: card.back_side().map(ToOwned::to_owned),
            class,
        };
        card.into_type(instance);
        Ok(())
    }

    pub fn set_dependency(&self, card_id: CardId, dependency: CardId) {
        if card_id == dependency {
            return;
        }

        let mut card = self.card_from_id(card_id);
        card.set_dependency(dependency);
        card.persist();
    }

    pub fn load_class_cards(&self) -> Vec<Card<AnyType>> {
        self.full_load_cards()
            .into_iter()
            .filter(|card| card.is_class())
            .collect()
    }

    pub fn load_pending(&self, filter: Option<String>) -> Vec<CardId> {
        self.full_load_cards()
            .into_iter()
            .filter(|card| card.history().is_empty())
            .filter(|card| {
                if let Some(ref filter) = filter {
                    card.eval(filter.clone())
                } else {
                    true
                }
            })
            .map(|card| card.id())
            .collect()
    }

    pub fn new_any(&self, any: impl Into<AnyType>) -> Card<AnyType> {
        let raw_card = new_raw_card(any);
        let id = raw_card.id;
        self.foobar.provider.save_card(raw_card);
        self.foobar.load_card(CardId(id)).unwrap()
    }
}

use crate::card::serializing::new_raw_card;

pub fn as_graph(app: &App) -> String {
    graphviz::export(app)
}

mod graphviz {
    use std::collections::BTreeSet;

    use super::*;

    pub fn export(app: &App) -> String {
        let mut dot = String::from("digraph G {\nranksep=2.0;\nrankdir=BT;\n");
        let mut relations = BTreeSet::default();
        let cards = app.full_load_cards();

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
