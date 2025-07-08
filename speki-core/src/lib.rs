use card::{BackSide, CardId, RawCard, TextData};
use card_provider::CardProvider;
use cardfilter::CardFilter;
use dioxus_logger::tracing::info;
use ledger::{CardAction, CardEvent};
use ledgerstore::Ledger;
use ledgerstore::TimeProvider;
use metadata::Metadata;
use recall_rate::History;
use set::Set;
use std::fmt::Display;
use std::path::PathBuf;
use std::{fmt::Debug, sync::Arc, time::Duration};
use tracing::trace;

pub mod audio;
pub mod card;
pub mod card_provider;
pub mod cardfilter;
pub mod collection;
mod common;
pub mod ledger;
pub mod metadata;
pub mod recall_rate;
pub mod set;

pub use card::{Card, CardType};
pub use common::current_time;
pub use omtrent::TimeStamp;
pub use recall_rate::SimpleRecall;

/// {from} is a(n) {ty} of {to}
#[derive(Clone, PartialEq, PartialOrd, Hash, Eq, Debug)]
pub enum CardRefType {
    ExplicitDependency,
    ClassOfInstance,
    LinkRef,
    ParentClass,
    InstanceOfAttribute,
}

impl Display for CardRefType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_ref())
    }
}

impl AsRef<str> for CardRefType {
    fn as_ref(&self) -> &str {
        match self {
            Self::ExplicitDependency => "explicit_dependency",
            Self::ClassOfInstance => "class_of_instance",
            Self::LinkRef => "linkref",
            Self::ParentClass => "parent_class",
            Self::InstanceOfAttribute => "instance_of_attribute",
        }
    }
}

impl CardRefType {
    pub fn to_str(&self) -> &str {
        self.as_ref()
    }
}

#[derive(Clone, PartialEq, PartialOrd, Hash, Eq, Debug)]
pub enum CardProperty {
    Trivial,
    Bigram,
    Suspended,
    CardType,
    AttrId,
    /// mapping of attributeid -> CardId
    Attr,
}

impl Display for CardProperty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_ref())
    }
}

impl AsRef<str> for CardProperty {
    fn as_ref(&self) -> &str {
        match self {
            CardProperty::Bigram => "bigram",
            CardProperty::Suspended => "suspended",
            CardProperty::CardType => "cardtype",
            CardProperty::AttrId => "attr_id",
            CardProperty::Attr => "attr",
            CardProperty::Trivial => "trivial",
        }
    }
}

#[derive(Copy, Clone)]
pub struct FsTime;

impl TimeProvider for FsTime {
    fn current_time(&self) -> Duration {
        Duration::from_secs(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        )
    }
}

#[derive(Clone)]
pub struct Provider {
    pub cards: Ledger<RawCard>,
    pub sets: Ledger<Set>,
    pub reviews: Ledger<History>,
    pub metadata: Ledger<Metadata>,
    pub time: FsTime,
}

pub struct App {
    pub provider: Provider,
    pub card_provider: CardProvider,
    pub time_provider: FsTime,
    pub recaller: SimpleRecall,
}

impl Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "app!")
    }
}

impl App {
    pub fn new(root: PathBuf) -> Self {
        info!("initialtize app");

        let provider = Provider {
            cards: Ledger::new(root.clone()),
            reviews: Ledger::new(root.clone()),
            metadata: Ledger::new(root.clone()),
            time: FsTime,
            sets: Ledger::new(root),
        };

        let card_provider = CardProvider::new(provider.clone(), FsTime, SimpleRecall);

        Self {
            provider,
            card_provider,
            time_provider: FsTime,
            recaller: SimpleRecall,
        }
    }

    pub fn card_provider(&self) -> CardProvider {
        self.card_provider.clone()
    }

    pub async fn fill_cache(&self) {
        info!("filling cache");
        let start = self.time_provider.current_time();
        let elapsed = self.time_provider.current_time() - start;
        info!("cache filled in {:.4} seconds!", elapsed.as_secs_f32());
    }

    pub fn load_all_cards(&self) -> Vec<Arc<Card>> {
        self.card_provider.load_all()
    }

    pub fn load_card_sync(&self, id: CardId) -> Option<Card> {
        trace!("loading card: {id}");
        let card = self.card_provider.load(id);
        trace!("card loaded i guess: {card:?}");
        Some(Arc::unwrap_or_clone(card?))
    }

    pub fn load_card(&self, id: CardId) -> Option<Card> {
        self.load_card_sync(id)
    }

    pub async fn load_cards(&self) -> Vec<CardId> {
        self.card_provider.load_all_card_ids()
    }

    pub fn cards_filtered(&self, filter: CardFilter) -> Vec<Arc<Card>> {
        let cards = self.load_all_cards();
        let mut ids = vec![];

        for card in cards {
            if filter.filter(card.clone()) {
                ids.push(card);
            }
        }
        ids
    }

    pub fn add_instance(
        &self,
        front: String,
        back: Option<impl Into<BackSide>>,
        class: CardId,
    ) -> CardId {
        let back = back.map(|back| back.into());
        let data = CardType::Instance {
            name: TextData::from_raw(&front),
            back,
            class,
        };
        let id = CardId::new_v4();
        let event = CardAction::UpsertCard(data);
        let event = CardEvent::new_modify(id, event);

        self.provider.cards.modify(event).unwrap();
        id
    }

    pub async fn add_card_with_id(&self, front: String, back: impl Into<BackSide>, id: CardId) {
        let back = back.into();
        let data = CardType::Normal {
            front: TextData::from_raw(&front),
            back,
        };
        let event = CardEvent::new_modify(id, CardAction::UpsertCard(data));
        self.provider.cards.modify(event).unwrap();
    }

    pub fn add_card(&self, front: String, back: impl Into<BackSide>) -> CardId {
        let back = back.into();
        let data = CardType::Normal {
            front: TextData::from_raw(&front),
            back,
        };

        let id = CardId::new_v4();
        let event = CardEvent::new_modify(id, CardAction::UpsertCard(data));
        self.provider.cards.modify(event).unwrap();
        id
    }

    pub async fn add_unfinished(&self, front: String) -> CardId {
        let data = CardType::Unfinished {
            front: TextData::from_raw(&front),
        };
        let id = CardId::new_v4();
        let event = CardEvent::new_modify(id, CardAction::UpsertCard(data));
        self.provider.cards.modify(event).unwrap();
        id
    }

    pub fn load_class_cards(&self) -> Vec<Arc<Card>> {
        self.load_all_cards()
            .into_iter()
            .filter(|card| card.is_class())
            .collect()
    }
}

pub fn as_graph(app: &App) -> String {
    graphviz::export(app)
}

pub mod graphviz {
    use std::collections::BTreeSet;

    use super::*;

    pub fn export_cards(cards: impl IntoIterator<Item = Arc<Card>>) -> String {
        let mut dot = String::from("digraph G {\nranksep=2.0;\nrankdir=BT;\n");
        let mut relations = BTreeSet::default();

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
                    let maturity = card.maturity_days().unwrap_or_default();
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
            for child_id in card.dependencies() {
                relations.insert(format!("    \"{}\" -> \"{}\";\n", card.id(), child_id));
            }
        }

        for rel in relations {
            dot.push_str(&rel);
        }

        dot.push_str("}\n");
        dot
    }

    pub fn export(app: &App) -> String {
        let cards = app.load_all_cards();
        export_cards(cards)
    }

    // Convert recall rate to a color, from red to green
    fn rate_to_color(rate: f64) -> String {
        let red = ((1.0 - rate / 100.0) * 255.0) as u8;
        let green = (rate / 100.0 * 255.0) as u8;
        format!("#{red:02X}{green:02X}00") // RGB color in hex
    }

    fn cyan_color() -> String {
        String::from("#00FFFF")
    }

    fn yellow_color() -> String {
        String::from("#FFFF00")
    }
}
