use std::{collections::{BTreeMap, BTreeSet}, sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use speki_dto::{Item, ModifiedSource, SpekiProvider};
use tracing::info;
use uuid::Uuid;

use crate::card::{BaseCard, CardId};

#[derive(Clone)]
pub struct DependentsProvider {
    inner: Arc<Box<dyn SpekiProvider<Dependents>>>,
}

impl DependentsProvider {
    pub fn new(inner: Arc<Box<dyn SpekiProvider<Dependents>>>) -> Self {
        Self {
            inner
        }
    }

    pub async fn load(&self, id: CardId) -> BTreeSet<CardId>{
        self.inner.load_item(id).await.map(|i|i.deps).unwrap_or_default()
    }

    async fn load_or_create(&self, id: CardId) -> Dependents {
        match self.inner.load_item(id).await {
            Some(idx) => idx,
            None => Dependents::new(id, Default::default(), Default::default()),
        }
    }

    pub async fn refresh(&self, cards: impl IntoIterator<Item = &BaseCard>) {
        info!("filling all dependents cache");

        let mut deps: BTreeMap<CardId, BTreeSet<CardId>> = BTreeMap::default();

        for card in cards {
            for dependency in card.dependencies().await {
                deps.entry(dependency).or_default().insert(card.id());
            }
        }

        for (id, deps) in deps {
            let dependents = Dependents::new(id, deps, Default::default());
            self.inner.save_item(dependents).await;
        }
        info!("done filling all dependents cache");
    }


    pub async fn update(&self, old_card: Option<&BaseCard>, new_card: &BaseCard) {
        let id = new_card.id;

        let new_dependencies = new_card.dependencies().await;
        let old_dependencies  = {
            if let Some(card)= 
            old_card {
                card.dependencies().await
            } else {
                Default::default()
            }
        };

        let removed_dependencies = old_dependencies.difference(&new_dependencies);
        let added_dependencies = new_dependencies.difference(&old_dependencies);

        for dependency in removed_dependencies {
            let mut dependents = self.load_or_create(*dependency).await;
            dependents.deps.remove(&id);
            self.inner.save_item(dependents).await;
        }

        for dependency in added_dependencies {
            let mut dependents = self.load_or_create(*dependency).await;
            dependents.deps.contains(&id);
            dependents.deps.insert(id);
            self.inner.save_item(dependents).await;
        }
    }
}



#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Dependents {
    id: CardId,
    pub deps: BTreeSet<CardId>,
    source: ModifiedSource,
    deleted: bool,
    last_modified: Duration,
}

impl Dependents {
    pub fn new(card: CardId, deps: BTreeSet<CardId>, current_time: Duration) -> Self {
        Self {
            id: card,
            deps,
            source: Default::default(),
            deleted: false,
            last_modified: current_time,
        }

    }
}


impl Item for Dependents {
    type PreviousVersion = Dependents;
    type Key = Uuid;

    fn deleted(&self) -> bool {
        false
    }

    fn set_delete(&mut self) {
        panic!("don't delete dependents cache!")
    }

    fn set_last_modified(&mut self, time: std::time::Duration) {
        self.last_modified = time;
    }

    fn last_modified(&self) -> std::time::Duration {
        self.last_modified
    }

    fn id(&self) -> uuid::Uuid {
        self.id
    }

    fn identifier() -> &'static str {
        "dependents"
    }

    fn source(&self) -> ModifiedSource {
        self.source
    }

    fn set_source(&mut self, source: ModifiedSource) {
        self.source = source;
    }
}