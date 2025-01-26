use std::{collections::HashMap, fmt::Debug, sync::Arc, time::Duration};

use audio::Audio;
use card::{BackSide, BaseCard, CardId, RecallRate};
use card_provider::CardProvider;
use cardfilter::{CardFilter, FilterItem};
use collection::{Collection, CollectionId, DynCard};
use dioxus_logger::tracing::info;
use eyre::Result;
use metadata::Metadata;
use recall_rate::History;
use speki_dto::{SpekiProvider, TimeProvider};
use tracing::trace;

mod attribute;
pub mod audio;
pub mod card;
mod card_provider;
pub mod cardfilter;
pub mod collection;
mod common;
pub mod healthcheck;
pub mod metadata;
pub mod recall_rate;

pub use attribute::{Attribute, AttributeDTO, AttributeId};
pub use card::{
    AttributeCard, Card, CardTrait, CardType, ClassCard, EventCard, InstanceCard, NormalCard,
    StatementCard, UnfinishedCard,
};
pub use common::current_time;
pub use omtrent::TimeStamp;
pub use recall_rate::SimpleRecall;

pub trait RecallCalc {
    fn recall_rate(&self, reviews: &History, current_unix: Duration) -> Option<RecallRate>;
}

#[derive(Clone)]
pub struct CollectionProvider {
    inner: Arc<Box<dyn SpekiProvider<Collection>>>,
}

impl CollectionProvider {
    pub async fn save(&self, collection: Collection) {
        self.inner.save_item(collection).await
    }

    pub async fn load(&self, id: CollectionId) -> Option<Collection> {
        if let Some(mut col) = self.inner.load_item(id).await {
            let mut dyns = vec![];
            for d in col.dyncards.clone() {
                if let DynCard::Collection(id) = d.clone() {
                    if self.inner.load_item(id).await.is_some() {
                        dyns.push(d);
                    }
                } else {
                    dyns.push(d);
                }
            }

            col.dyncards = dyns;
            Some(col)
        } else {
            None
        }
    }

    pub async fn load_all(&self) -> HashMap<CollectionId, Collection> {
        self.inner.load_all().await
    }

    pub async fn delete(&self, item: Collection) {
        self.inner.delete_item(item).await
    }
}

#[derive(Clone)]
pub struct Provider {
    pub cards: Arc<Box<dyn SpekiProvider<BaseCard>>>,
    pub reviews: Arc<Box<dyn SpekiProvider<History>>>,
    pub attrs: Arc<Box<dyn SpekiProvider<AttributeDTO>>>,
    pub collections: CollectionProvider,
    pub metadata: Arc<Box<dyn SpekiProvider<Metadata>>>,
    pub cardfilter: Arc<Box<dyn SpekiProvider<FilterItem>>>,
    pub audios: Arc<Box<dyn SpekiProvider<Audio>>>,
}

pub type Recaller = Arc<Box<dyn RecallCalc + Send>>;
pub type TimeGetter = Arc<Box<dyn TimeProvider + Send>>;

pub struct App {
    pub provider: Provider,
    pub card_provider: CardProvider,
    pub time_provider: TimeGetter,
    pub recaller: Recaller,
}

impl Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "app!")
    }
}

impl App {
    pub fn new<A, B, C, D, E, F, G, H, I>(
        recall_calc: A,
        time_provider: B,
        card_provider: C,
        history_provider: D,
        attr_provider: E,
        collections_provider: F,
        meta_provider: G,
        filter_provider: H,
        audio_provider: I,
    ) -> Self
    where
        A: RecallCalc + 'static + Send,
        B: TimeProvider + 'static + Send,
        C: SpekiProvider<BaseCard> + 'static + Send,
        D: SpekiProvider<History> + 'static + Send,
        E: SpekiProvider<AttributeDTO> + 'static + Send,
        F: SpekiProvider<Collection> + 'static + Send,
        G: SpekiProvider<Metadata> + 'static + Send,
        H: SpekiProvider<FilterItem> + 'static + Send,
        I: SpekiProvider<Audio> + 'static + Send,
    {
        info!("initialtize app");

        let time_provider: TimeGetter = Arc::new(Box::new(time_provider));
        let recaller: Recaller = Arc::new(Box::new(recall_calc));

        let provider = Provider {
            cards: Arc::new(Box::new(card_provider)),
            reviews: Arc::new(Box::new(history_provider)),
            attrs: Arc::new(Box::new(attr_provider)),
            collections: CollectionProvider {
                inner: Arc::new(Box::new(collections_provider)),
            },
            metadata: Arc::new(Box::new(meta_provider)),
            cardfilter: Arc::new(Box::new(filter_provider)),
            audios: Arc::new(Box::new(audio_provider)),
        };

        let card_provider =
            CardProvider::new(provider.clone(), time_provider.clone(), recaller.clone());

        Self {
            provider,
            card_provider,
            time_provider,
            recaller,
        }
    }

    pub fn card_provider(&self) -> CardProvider {
        self.card_provider.clone()
    }

    pub async fn fill_cache(&self) {
        info!("filling cache");
        let start = self.time_provider.current_time();
        self.card_provider.fill_cache().await;
        let elapsed = self.time_provider.current_time() - start;
        info!("cache filled in {:.4} seconds!", elapsed.as_secs_f32());
    }

    pub async fn load_all_cards(&self) -> Vec<Arc<Card>> {
        self.card_provider.load_all().await
    }

    pub async fn save_card_not_reviews(&self, card: Card) {
        self.card_provider.save_card(card).await;
    }

    pub async fn save_card(&self, card: Card) {
        self.card_provider
            .save_reviews(card.history().clone())
            .await;

        self.card_provider.save_card(card).await;
    }

    pub async fn health_check(&self) {
        healthcheck::healthcheck(self.card_provider.clone()).await;
    }

    pub async fn load_card(&self, id: CardId) -> Option<Card> {
        trace!("loading card: {id}");
        let card = self.card_provider.load(id).await;
        trace!("card loaded i guess: {card:?}");
        Some(Arc::unwrap_or_clone(card?))
    }

    pub async fn load_cards(&self) -> Vec<CardId> {
        self.card_provider.load_all_card_ids().await
    }

    pub async fn load_and_persist(&self) {
        for card in self.load_all_cards().await {
            Arc::unwrap_or_clone(card).persist().await;
        }
    }

    pub async fn cards_filtered(&self, filter: CardFilter) -> Vec<Arc<Card>> {
        let cards = self.load_all_cards().await;
        let mut ids = vec![];

        for card in cards {
            if filter.filter(card.clone()).await {
                ids.push(card);
            }
        }
        ids
    }

    pub async fn add_class(
        &self,
        front: String,
        back: impl Into<BackSide>,
        parent_class: Option<CardId>,
    ) -> CardId {
        let back = back.into();
        let data = ClassCard {
            name: front,
            back,
            parent_class,
        };

        let base = BaseCard::new(data);
        self.card_provider().save_basecard(base).await.id()
    }

    pub async fn add_instance(
        &self,
        front: String,
        back: Option<impl Into<BackSide>>,
        class: CardId,
    ) -> CardId {
        let back = back.map(|back| back.into());
        let data = InstanceCard {
            name: front,
            back,
            class,
        };
        let base = BaseCard::new(data);
        self.card_provider().save_basecard(base).await.id()
    }

    pub async fn add_card_with_id(&self, front: String, back: impl Into<BackSide>, id: CardId) {
        let back = back.into();
        let data = NormalCard { front, back };
        let card = BaseCard::new_with_id(id, data);
        self.provider.cards.save_item(card).await;
    }

    pub async fn add_card(&self, front: String, back: impl Into<BackSide>) -> CardId {
        let back = back.into();
        let data = NormalCard { front, back };
        let base = BaseCard::new(data);
        self.card_provider().save_basecard(base).await.id()
    }

    pub async fn add_unfinished(&self, front: String) -> CardId {
        let data = UnfinishedCard { front };
        let base = BaseCard::new(data);
        self.card_provider().save_basecard(base).await.id()
    }

    pub async fn set_class(&self, card_id: CardId, class: CardId) -> Result<()> {
        let card = self.card_provider.load(card_id).await.unwrap();

        let instance = InstanceCard {
            name: card.print().await,
            back: card.back_side().map(ToOwned::to_owned),
            class,
        };
        Arc::unwrap_or_clone(card).into_type(instance).await;
        Ok(())
    }

    pub async fn load_class_cards(&self) -> Vec<Arc<Card>> {
        self.load_all_cards()
            .await
            .into_iter()
            .filter(|card| card.is_class())
            .collect()
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
