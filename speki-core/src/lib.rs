use card::serializing::from_raw_card;
use card::RecallRate;
use card_provider::CardProvider;
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
mod card_provider;
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

pub trait TimeProvider {
    fn current_time(&self) -> Duration;
}

pub type Provider = Arc<Box<dyn SpekiProvider + Send>>;
pub type Recaller = Arc<Box<dyn RecallCalc + Send>>;
pub type TimeGetter = Arc<Box<dyn TimeProvider + Send>>;

pub struct App {
    pub card_provider: CardProvider,
    pub time_provider: TimeGetter,
    pub recaller: Recaller,
    pub provider: Provider,
}

impl Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "app!")
    }
}

impl App {
    pub fn new<A, B, C>(provider: A, recall_calc: B, time_provider: C) -> Self
    where
        A: SpekiProvider + 'static + Send,
        B: RecallCalc + 'static + Send,
        C: TimeProvider + 'static + Send,
    {
        let time_provider: TimeGetter = Arc::new(Box::new(time_provider));
        let recaller: Recaller = Arc::new(Box::new(recall_calc));
        let provider: Provider = Arc::new(Box::new(provider));

        let card_provider =
            CardProvider::new(provider.clone(), time_provider.clone(), recaller.clone());

        Self {
            card_provider,
            time_provider,
            recaller,
            provider,
        }
    }

    pub async fn load_all_cards(&self) -> Vec<Card<AnyType>> {
        let card_provider = self.card_provider.clone();
        let raw_cards = self.provider.load_all_cards().await;

        let tasks = raw_cards.into_iter().map(|card| {
            let card_provider = card_provider.clone();
            let recaller = card_provider.recaller();
            async move { Card::from_raw(card, card_provider, recaller).await }
        });

        futures::future::join_all(tasks).await
    }

    pub async fn save_card(&self, card: Card<AnyType>) {
        self.provider.save_card(from_raw_card(card)).await;
    }

    pub async fn load_card(&self, id: CardId) -> Option<Card<AnyType>> {
        let card_provider = self.card_provider.clone();
        let raw = self.provider.load_card(id).await?;
        let recaller = card_provider.recaller();
        Some(Card::from_raw(raw, card_provider, recaller).await)
    }

    pub async fn delete_card(&self, id: CardId) {
        self.provider.delete_card(id).await;
    }

    pub async fn load_all_attributes(&self, card_provider: CardProvider) -> Vec<Attribute> {
        self.provider
            .load_all_attributes()
            .await
            .into_iter()
            .map(|dto| Attribute::from_dto(dto, card_provider.clone()))
            .collect()
    }

    pub async fn save_attribute(&self, attribute: Attribute) {
        self.provider
            .save_attribute(Attribute::into_dto(attribute))
            .await;
    }

    pub async fn load_attribute(&self, id: AttributeId) -> Option<Attribute> {
        let card_provider = self.card_provider.clone();
        self.provider
            .load_attribute(id)
            .await
            .map(|dto| Attribute::from_dto(dto, card_provider.clone()))
    }

    pub async fn delete_attribute(&self, id: AttributeId) {
        self.provider.delete_attribute(id).await;
    }

    pub async fn load_reviews(&self, id: CardId) -> Reviews {
        Reviews(self.provider.load_reviews(id).await)
    }

    pub async fn save_reviews(&self, id: CardId, reviews: Reviews) {
        self.provider.save_reviews(id, reviews.into_inner()).await;
    }

    pub async fn add_review(&self, id: CardId, review: Review) {
        self.provider.add_review(id, review).await;
    }

    pub fn load_config(&self) -> Config {
        Config
    }

    pub fn save_config(&self, _config: Config) {}

    pub async fn load_cards(&self) -> Vec<CardId> {
        self.load_all_cards()
            .await
            .into_iter()
            .map(|card| card.id())
            .collect()
    }

    async fn full_load_cards(&self) -> Vec<Card<AnyType>> {
        self.load_all_cards().await
    }

    pub async fn load_non_pending(&self, filter: Option<String>) -> Vec<CardId> {
        let iter = self
            .full_load_cards()
            .await
            .into_iter()
            .filter(|card| !card.history().is_empty());

        let mut ids = vec![];

        if let Some(ref filter) = filter {
            for card in iter {
                if card.eval(filter.clone()).await {
                    ids.push(card.id());
                }
            }
        } else {
            for card in iter {
                ids.push(card.id());
            }
        }

        ids
    }

    pub async fn card_from_id(&self, id: CardId) -> Card<AnyType> {
        self.card_provider.load(id).await.unwrap()
    }

    pub async fn load_and_persist(&self) {
        for mut card in self.full_load_cards().await {
            card.persist().await;
        }
    }

    pub fn get_cached_dependents(&self, id: CardId) -> BTreeSet<CardId> {
        Card::<AnyType>::dependents(id)
    }

    pub async fn cards_filtered(&self, filter: String) -> Vec<CardId> {
        let cards = self.full_load_cards().await;
        let mut ids = vec![];

        for card in cards {
            if card.eval(filter.clone()).await {
                ids.push(card.id());
            }
        }
        ids
    }

    pub async fn add_card(&self, front: String, back: String) -> CardId {
        let data = NormalCard {
            front,
            back: back.into(),
        };
        self.new_any(data).await.id()
    }

    pub async fn add_unfinished(&self, front: String) -> CardId {
        let data = UnfinishedCard { front };
        self.new_any(data).await.id()
    }

    pub async fn review(&self, id: CardId, grade: Recall) {
        let review = Review {
            timestamp: current_time(),
            grade,
            time_spent: Default::default(),
        };
        self.add_review(id, review).await;
    }

    pub async fn set_class(&self, card_id: CardId, class: CardId) -> Result<()> {
        let card = self.card_from_id(card_id).await;

        let instance = InstanceCard {
            name: card.card_type().display_front().await,
            back: card.back_side().map(ToOwned::to_owned),
            class,
        };
        card.into_type(instance).await;
        Ok(())
    }

    pub async fn set_dependency(&self, card_id: CardId, dependency: CardId) {
        if card_id == dependency {
            return;
        }

        let mut card = self.card_from_id(card_id).await;
        card.set_dependency(dependency).await;
        card.persist().await;
    }

    pub async fn load_class_cards(&self) -> Vec<Card<AnyType>> {
        self.full_load_cards()
            .await
            .into_iter()
            .filter(|card| card.is_class())
            .collect()
    }

    pub async fn load_pending(&self, filter: Option<String>) -> Vec<CardId> {
        let iter = self
            .full_load_cards()
            .await
            .into_iter()
            .filter(|card| card.history().is_empty());

        let mut ids = vec![];

        if let Some(ref filter) = filter {
            for card in iter {
                if card.eval(filter.clone()).await {
                    ids.push(card.id());
                }
            }
        } else {
            for card in iter {
                ids.push(card.id());
            }
        }

        ids
    }

    pub async fn new_any(&self, any: impl Into<AnyType>) -> Card<AnyType> {
        let raw_card = new_raw_card(any);
        let id = raw_card.id;
        self.provider.save_card(raw_card).await;
        self.card_provider.load(CardId(id)).await.unwrap()
    }
}

use crate::card::serializing::new_raw_card;

pub async fn as_graph(app: &App) -> String {
    graphviz::export(app).await
}

mod graphviz {
    use std::collections::BTreeSet;

    use super::*;

    pub async fn export(app: &App) -> String {
        let mut dot = String::from("digraph G {\nranksep=2.0;\nrankdir=BT;\n");
        let mut relations = BTreeSet::default();
        let cards = app.full_load_cards().await;

        for card in cards {
            let label = card
                .print()
                .await
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
            for child_id in card.dependency_ids().await {
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
