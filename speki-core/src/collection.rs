use std::{collections::BTreeSet, fmt::Display, sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use speki_dto::{Item, ModifiedSource};
use uuid::Uuid;

use crate::{card::CardId, card_provider::CardProvider, Card};

pub type CollectionId = Uuid;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Collection {
    pub id: Uuid,
    pub name: String,
    pub dyncards: Vec<DynCard>,
    pub last_modified: Duration,
    pub deleted: bool,
    pub source: ModifiedSource,
}

impl Collection {
    pub fn new(name: String) -> Self {
        Self {
            id: CollectionId::new_v4(),
            name,
            dyncards: Default::default(),
            last_modified: Default::default(),
            deleted: Default::default(),
            source: Default::default(),
        }
    }

    pub async fn expand(&self, provider: CardProvider) -> Vec<Arc<Card>> {
        let mut out = BTreeSet::new();

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

        out.into_iter().collect()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub enum DynCard {
    Card(CardId),
    Instances(CardId),
    Dependents(CardId),
    RecDependents(CardId),
}

impl DynCard {
    pub fn id(&self) -> CardId {
        *match self {
            DynCard::Card(id) => id,
            DynCard::Instances(id) => id,
            DynCard::Dependents(id) => id,
            DynCard::RecDependents(id) => id,
        }
    }

    pub async fn evaluate(&self, provider: CardProvider) -> Vec<Arc<Card>> {
        match self {
            DynCard::Card(id) => {
                let card = provider.load(*id).await.unwrap();
                vec![card]
            }
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
