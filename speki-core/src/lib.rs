use crate::card::serializing::new_raw_card;
use attribute::AttrProvider;
use card::filter_rec_recall;
use card::RecallRate;
use card_provider::CardProvider;
use eyre::Result;
use reviews::Reviews;
use samsvar::Matcher;
use samsvar::Schema;
use speki_dto::AttributeId;
use speki_dto::SpekiProvider;
use std::collections::HashMap;
use std::fmt::Debug;
use std::time::Duration;
use tracing::instrument;

//pub mod collections;
//pub mod github;
//mod attr_provider;
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
    pub attr_provider: AttrProvider,
}

impl Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "app!")
    }
}

use dioxus_logger::tracing::info;

impl App {
    pub fn new<A, B, C>(provider: A, recall_calc: B, time_provider: C) -> Self
    where
        A: SpekiProvider + 'static + Send,
        B: RecallCalc + 'static + Send,
        C: TimeProvider + 'static + Send,
    {
        info!("initialtize app");

        let time_provider: TimeGetter = Arc::new(Box::new(time_provider));
        let recaller: Recaller = Arc::new(Box::new(recall_calc));
        let provider: Provider = Arc::new(Box::new(provider));

        let card_provider =
            CardProvider::new(provider.clone(), time_provider.clone(), recaller.clone());

        let attr_provider = AttrProvider::new(provider.clone(), card_provider.clone());

        Self {
            card_provider,
            time_provider,
            recaller,
            attr_provider,
        }
    }

    pub async fn fill_cache(&self) {
        info!("filling cache");
        self.card_provider.load_all().await;
        info!("cache filled!");
    }

    #[instrument]
    pub async fn load_all_cards(&self) -> Vec<Arc<Card<AnyType>>> {
        self.card_provider.load_all().await
    }

    pub async fn save_card(&self, card: Card<AnyType>) {
        self.card_provider.save_card(card).await;
    }

    pub async fn load_card(&self, id: CardId) -> Option<Card<AnyType>> {
        Arc::unwrap_or_clone(self.card_provider.load(id).await?).into()
    }

    pub async fn delete_card(&self, id: CardId) {
        self.card_provider.delete_card(id).await;
    }

    pub async fn load_all_attributes(&self) -> Vec<Attribute> {
        self.attr_provider.load_all().await
    }

    pub async fn save_attribute(&self, attribute: Attribute) {
        self.attr_provider.save(attribute).await;
    }

    pub async fn load_attribute(&self, id: AttributeId) -> Option<Attribute> {
        self.attr_provider.load(id).await
    }

    pub async fn delete_attribute(&self, id: AttributeId) {
        self.attr_provider.delete(id).await
    }

    pub async fn load_cards(&self) -> Vec<CardId> {
        self.card_provider.load_all_card_ids().await
    }

    #[instrument]
    pub async fn load_non_pending(&self, filter: Option<String>) -> Vec<CardId> {
        info!("loading card ids");
        let cards: HashMap<_, _> = self
            .load_all_cards()
            .await
            .into_iter()
            .filter(|card| !card.history().is_empty())
            .map(|card| (card.id, card))
            .collect();

        info!("starting filter recall thing");
        let cards = filter_rec_recall(cards, 0.8).await;

        let mut ids = vec![];

        if let Some(ref filter) = filter {
            let schema = Schema::new(filter.clone()).unwrap();

            info!("applying filters..");
            for card in cards {
                info!("applying filter");
                if card.eval_schema(&schema).await {
                    //    info!("filter match");
                    ids.push(card.id());
                } else {
                    //   info!("filter not match");
                }
            }
        } else {
            for card in cards {
                ids.push(card.id());
            }
        }

        ids
    }

    pub async fn load_and_persist(&self) {
        for card in self.load_all_cards().await {
            Arc::unwrap_or_clone(card).persist().await;
        }
    }

    pub async fn cards_filtered(&self, filter: String) -> Vec<CardId> {
        let cards = self.load_all_cards().await;
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

    pub async fn set_class(&self, card_id: CardId, class: CardId) -> Result<()> {
        let card = self.card_provider.load(card_id).await.unwrap();

        let instance = InstanceCard {
            name: card.card_type().display_front().await,
            back: card.back_side().map(ToOwned::to_owned),
            class,
        };
        Arc::unwrap_or_clone(card).into_type(instance).await;
        Ok(())
    }

    pub async fn load_class_cards(&self) -> Vec<Arc<Card<AnyType>>> {
        self.load_all_cards()
            .await
            .into_iter()
            .filter(|card| card.is_class())
            .collect()
    }

    pub async fn load_pending(&self, filter: Option<String>) -> Vec<CardId> {
        let iter = self
            .load_all_cards()
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
        let card =
            Card::from_raw(raw_card, self.card_provider.clone(), self.recaller.clone()).await;

        self.card_provider.save_card(card).await;
        Arc::unwrap_or_clone(self.card_provider.load(CardId(id)).await.unwrap())
    }
}

pub async fn as_graph(app: &App) -> String {
    graphviz::export(app).await
}

mod graphviz {
    use std::collections::BTreeSet;

    use super::*;

    pub async fn export(app: &App) -> String {
        let mut dot = String::from("digraph G {\nranksep=2.0;\nrankdir=BT;\n");
        let mut relations = BTreeSet::default();
        let cards = app.load_all_cards().await;

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
