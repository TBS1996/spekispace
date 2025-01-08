use std::{collections::BTreeSet, sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use speki_dto::{Item, ModifiedSource};
use uuid::Uuid;

use crate::{card::CardId, card_provider::CardProvider, Card};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Collection {
    pub id: Uuid,
    pub name: String,
    pub cards: Vec<CardId>,
    pub dyncards: Vec<DynCard>,
    pub last_modified: Duration,
    pub deleted: bool,
    pub source: ModifiedSource,
}

impl Collection {
    pub async fn expand(&self, provider: CardProvider) -> BTreeSet<Arc<Card>> {
        let mut out = BTreeSet::new();

        for card in self.cards.clone() {
            let card = provider.load(card).await.unwrap();
            out.insert(card.clone());
        }

        for dyncard in &self.dyncards {
            for card in dyncard.evaluate(provider.clone()).await {
                out.insert(card);
            }
        }

        let mut dependencies = BTreeSet::new();

        for card in &out {
            for dep in card.all_dependencies().await {
                let card = provider.load(dep).await.unwrap();
                dependencies.insert(card);
            }
        }

        out.extend(dependencies);

        out
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum DynCard {
    Instances(CardId),
    Dependents(CardId),
    RecDependents(CardId),
}

impl DynCard {
    pub async fn evaluate(&self, provider: CardProvider) -> Vec<Arc<Card>> {
        match self {
            DynCard::Instances(id) => {
                let card = provider.load(*id).await.unwrap();
                let mut output = vec![];

                for card in card.dependents().await {
                    if card.is_instance_of(*id) {
                        output.push(card);
                    }
                }

                output
            }
            DynCard::Dependents(id) => provider
                .load(*id)
                .await
                .unwrap()
                .dependents()
                .await
                .into_iter()
                .collect(),
            DynCard::RecDependents(id) => {
                let ids = provider.load(*id).await.unwrap().all_dependents().await;
                let mut out = vec![];

                for id in ids {
                    let card = provider.load(id).await.unwrap();
                    out.push(card);
                }

                out
            }
        }
    }
}

impl Item for Collection {
    fn deleted(&self) -> bool {
        self.deleted
    }

    fn set_delete(&mut self) {
        self.deleted = true;
    }

    fn set_last_modified(&mut self, time: Duration) {
        self.last_modified = time;
    }

    fn last_modified(&self) -> Duration {
        self.last_modified
    }

    fn id(&self) -> Uuid {
        self.id
    }

    fn identifier() -> &'static str {
        "collections"
    }

    fn source(&self) -> ModifiedSource {
        self.source
    }

    fn set_source(&mut self, source: ModifiedSource) {
        self.source = source;
    }
}
