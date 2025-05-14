use core::f32;
use std::{
    cmp::{Ord, Ordering, PartialEq},
    collections::{BTreeMap, BTreeSet, HashSet},
    fmt::Debug,
    sync::Arc,
    time::Duration,
};

use serde::Deserializer;
use serde_json::Value;
use tracing::info;
use uuid::Uuid;

use crate::{
    audio::{Audio, AudioId},
    card_provider::CardProvider,
    ledger::{CardAction, CardEvent, MetaEvent},
    metadata::Metadata,
    recall_rate::{History, Recall, ReviewEvent, SimpleRecall},
    RecallCalc, Recaller, RefType, TimeGetter,
};

pub type RecallRate = f32;

mod basecard;

pub use basecard::*;

#[derive(Clone)]
pub struct Card {
    id: CardId,
    front_audio: Option<Audio>,
    back_audio: Option<Audio>,
    frontside: String,
    backside: String,
    base: RawCard,
    metadata: Metadata,
    history: History,
    card_provider: CardProvider,
    recaller: Recaller,
}

impl PartialEq for Card {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Ord for Card {
    fn cmp(&self, other: &Self) -> Ordering {
        self.id.cmp(&other.id)
    }
}

impl PartialOrd for Card {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.id.partial_cmp(&other.id)
    }
}

impl Eq for Card {}

impl Debug for Card {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = String::new();
        s.push_str(&format!("{:?}\n", self.id));
        s.push_str(&format!("{:?}\n", self.base.data.type_name()));
        s.push_str(&format!("{:?}\n", self.base.data.raw_front()));

        write!(f, "{}", s)
    }
}

impl std::fmt::Display for Card {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.print())
    }
}

impl Card {
    pub fn clone_base(&self) -> RawCard {
        self.base.clone()
    }

    pub fn front_audio(&self) -> Option<&Audio> {
        self.front_audio.as_ref()
    }

    pub fn back_audio(&self) -> Option<&Audio> {
        self.back_audio.as_ref()
    }

    pub fn front_audio_id(&self) -> Option<AudioId> {
        self.base.front_audio
    }

    pub fn back_audio_id(&self) -> Option<AudioId> {
        self.base.back_audio
    }

    pub fn card_type(&self) -> CType {
        self.base.data.fieldless()
    }

    pub fn dependents_ids(&self) -> BTreeSet<CardId> {
        let id = self.id;
        let mut stack = BTreeSet::new();
        for dep in self
            .card_provider
            .providers
            .cards
            .get_ref_cache(RefType::Dependent, id)
        {
            let dep: CardId = dep.parse().unwrap();
            stack.insert(dep);
        }
        stack
    }

    pub fn dependents(&self) -> BTreeSet<Arc<Self>> {
        let mut cards = BTreeSet::default();
        for card in self.card_provider.dependents(self.id) {
            let card = self.card_provider.load(card).unwrap();
            cards.insert(card);
        }
        cards
    }

    pub async fn add_review(&mut self, recall: Recall) {
        let event = ReviewEvent {
            id: self.id,
            grade: recall,
            timestamp: self.current_time(),
        };
        self.card_provider.providers.run_event(event);
    }

    pub fn time_provider(&self) -> TimeGetter {
        self.card_provider.time_provider()
    }

    pub fn lapses_last_month(&self) -> u32 {
        let current_time = self.time_provider().current_time();
        let day = Duration::from_secs(86400 * 30);

        self.history.lapses_since(day, current_time)
    }
    pub fn lapses_last_week(&self) -> u32 {
        let current_time = self.time_provider().current_time();
        let day = Duration::from_secs(86400 * 7);

        self.history.lapses_since(day, current_time)
    }

    pub fn lapses_last_day(&self) -> u32 {
        let current_time = self.time_provider().current_time();
        let day = Duration::from_secs(86400);

        self.history.lapses_since(day, current_time)
    }

    pub fn from_parts(
        base: RawCard,
        history: History,
        metadata: Metadata,
        card_provider: CardProvider,
        recaller: Recaller,
        front_audio: Option<Audio>,
        back_audio: Option<Audio>,
    ) -> Self {
        let id = base.id;

        let raw_front = |id: Uuid| -> String {
            card_provider
                .providers
                .cards
                .load(&id.to_string())
                .unwrap()
                .data
                .raw_front()
        };

        let from_back = |back: &BackSide| -> String {
            match back {
                BackSide::Text(s) => s.clone(),
                BackSide::Card(id) => raw_front(*id),
                BackSide::List(ids) => {
                    let mut out = format!("-> [");

                    for id in ids {
                        let s = raw_front(*id);
                        out.push_str(&s);
                        out.push_str(", ");
                    }
                    out.pop();
                    out.pop();
                    out.push_str("]");
                    out
                }
                BackSide::Time(ts) => ts.to_string(),
                BackSide::Trivial => "<trivial>".to_string(),
                BackSide::Invalid => "<invalid>".to_string(),
            }
        };

        let backside = match &base.data {
            CardType::Instance { back, class, .. } => match back.as_ref() {
                Some(back) => from_back(&back),
                None => raw_front(*class),
            },
            CardType::Normal { back, .. } => from_back(&back),
            CardType::Unfinished { .. } => "<unfinished>".to_string(),
            CardType::Attribute { .. } => "<attribute>".to_string(),
            CardType::Class {
                back, parent_class, ..
            } => match (back, parent_class) {
                (Some(theback), Some(pcl)) if theback.is_empty_text() => card_provider
                    .providers
                    .cards
                    .load(pcl.to_string().as_str())
                    .unwrap()
                    .data
                    .raw_front(),
                (None, Some(pcl)) => card_provider
                    .providers
                    .cards
                    .load(pcl.to_string().as_str())
                    .unwrap()
                    .data
                    .raw_front(),
                (Some(back), None) => from_back(&back),
                (_, _) => format!(""),
            },
            CardType::Statement { .. } => "<statement>".to_string(),
            CardType::Event { .. } => "<event>".to_string(),
        };

        let frontside = base.data.display_front(&card_provider);

        Self {
            id,
            frontside,
            base,
            backside,
            metadata,
            history,
            card_provider,
            recaller,
            front_audio,
            back_audio,
        }
    }

    pub fn is_finished(&self) -> bool {
        !matches!(&self.base.data, CardType::Unfinished { .. })
    }

    pub fn is_class(&self) -> bool {
        matches!(&self.base.data, CardType::Class { .. })
    }

    pub fn is_instance_of(&self, _class: CardId) -> bool {
        if let CardType::Instance { class, .. } = &self.base.data {
            *class == _class
        } else {
            false
        }
    }

    pub async fn set_ref(mut self, reff: CardId) -> Card {
        let backside = BackSide::Card(reff);
        self.base = self.base.set_backside(backside);
        let action = CardAction::SetBackRef(reff);
        let event = CardEvent::new(self.id, action);
        self.card_provider.providers.run_event(event);
        self
    }

    pub async fn rm_dependency(&mut self, dependency: CardId) {
        info!(
            "for removal, dependent: {}, -- dependency: {}",
            self.id(),
            dependency
        );
        let res = self.base.dependencies.remove(&dependency);

        if !res {
            info!("no dep to remove");
            return;
        }

        info!("dep was there: {res}");
        self.base.remove_dep(dependency);
        let action = CardAction::RemoveDependency(dependency);
        let event = CardEvent::new(self.id, action);
        self.card_provider.providers.run_event(event);
    }

    pub async fn add_dependency(&mut self, dependency: CardId) {
        self.base.dependencies.insert(dependency);
        let action = CardAction::AddDependency(dependency);
        let event = CardEvent::new(self.id, action);
        self.card_provider.providers.run_event(event);
    }

    pub fn back_side(&self) -> Option<&BackSide> {
        self.base.data.backside()
    }

    pub async fn delete_card(self) {
        self.card_provider.remove_card(self.id).await;
    }

    pub fn recursive_dependents(&self) -> Vec<CardId> {
        use std::collections::VecDeque;

        let mut deps = vec![];
        let mut visited: HashSet<CardId> = Default::default();
        let mut stack = VecDeque::new();
        stack.push_back((self.id(), vec![self.id()]));

        while let Some((id, path)) = stack.pop_back() {
            if visited.contains(&id) {
                continue;
            }

            if self.id() != id {
                visited.insert(id);
                deps.push(id);
            }

            for dep_str in self
                .card_provider
                .providers
                .cards
                .get_ref_cache(RefType::Dependent, id)
            {
                let dep: CardId = dep_str.parse().unwrap();

                if path.contains(&dep) {
                    panic!(
                        "Cycle detected: {}",
                        path.iter()
                            .chain(std::iter::once(&dep))
                            .map(|id| format!(
                                "({id}: {})",
                                self.card_provider.load(*id).unwrap().base.data.raw_front()
                            ))
                            .collect::<Vec<_>>()
                            .join(" -> ")
                    );
                }

                let mut new_path = path.clone();
                new_path.push(dep);
                stack.push_back((dep, new_path));
            }
        }

        deps
    }

    pub async fn recursive_dependencies(&self) -> Vec<CardId> {
        tracing::trace!("getting dependencies of: {}", self.id);
        let mut deps = vec![];
        let mut stack = vec![self.id()];

        while let Some(id) = stack.pop() {
            let Some(card) = self.card_provider.load(id) else {
                continue;
            };

            if self.id() != id {
                deps.push(id);
            }

            for dep in card.dependencies() {
                stack.push(dep);
            }
        }

        deps
    }

    pub async fn min_rec_recall_rate(&self) -> RecallRate {
        tracing::trace!("min rec recall of {}", self.id);
        let mut min_recall: RecallRate = 1.0;

        for card in self.recursive_dependencies().await {
            let card = self.card_provider.load(card).unwrap();
            min_recall = min_recall.min(card.recall_rate().unwrap_or_default());
        }

        min_recall
    }

    pub fn display_backside(&self) -> &str {
        &self.backside
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    fn current_time(&self) -> Duration {
        self.card_provider.time_provider().current_time()
    }

    fn time_passed_since_last_review(&self) -> Option<Duration> {
        self.history.time_since_last_review(self.current_time())
    }

    pub fn recall_rate_at(&self, current_unix: Duration) -> Option<RecallRate> {
        SimpleRecall.recall_rate(&self.history, current_unix)
    }

    pub fn recall_rate(&self) -> Option<RecallRate> {
        let now = self.current_time();
        self.recaller.recall_rate(&self.history, now)
    }

    pub fn maturity(&self) -> Option<f32> {
        use gkquad::single::integral;

        let now = self.current_time();
        let result = integral(
            |x: f64| {
                self.recall_rate_at(now + Duration::from_secs_f64(x * 86400.))
                    .unwrap_or_default() as f64
            },
            0.0..1000.,
        )
        .estimate()
        .ok()?;

        Some(result as f32)
    }

    pub fn print(&self) -> String {
        self.frontside.clone()
    }

    pub fn is_pending(&self) -> bool {
        self.history.is_empty()
    }

    pub fn is_suspended(&self) -> bool {
        self.metadata.suspended.is_suspended()
    }

    pub async fn set_suspend(&mut self, suspend: bool) {
        let event = MetaEvent {
            id: self.id,
            action: crate::ledger::MetaAction::Suspend(suspend),
        };

        self.card_provider.providers.run_event(event);
        self.metadata.suspended = suspend.into();
    }

    pub fn time_since_last_review(&self) -> Option<Duration> {
        self.time_passed_since_last_review()
    }

    pub fn id(&self) -> CardId {
        self.id
    }

    pub fn dependencies(&self) -> BTreeSet<CardId> {
        self.base.dependencies()
    }

    pub fn lapses(&self) -> u32 {
        self.history.lapses()
    }
}
