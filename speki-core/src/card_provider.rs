use crate::{
    audio::Audio,
    card::{CardId, RawCard},
    collection::DynCard,
    ledger::{CardEvent, Event, MetaEvent},
    metadata::Metadata,
    recall_rate::{History, ReviewEvent},
    set::{Set, SetEvent, SetExpr},
    ArcRecall, Card, CardProperty, CardRefType, FsTime, MyEventError, Provider,
};
use dioxus_logger::tracing::{info, trace};
use indexmap::IndexSet;
use ledgerstore::{
    entry_thing::EventNode, EventError, ItemAction, ItemExpr, LedgerEvent, PropertyCache,
    ReadLedger,
};
use nonempty::NonEmpty;
use std::{
    collections::HashMap,
    fmt::Debug,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

pub fn event_nodes(
    provider: &impl ReadLedger<Item = RawCard>,
) -> Result<Vec<EventNode<RawCard>>, std::io::Error> {
    let cards = provider.load_expr(ItemExpr::All);
    let cards: Vec<CardId> = cards.into_iter().collect();
    let ordered = provider.topological_sort(cards);
    let qty = ordered.len();

    let mut event_nodes: Vec<EventNode<RawCard>> = vec![];

    // Each card's events live in its own node
    for (idx, card) in ordered.into_iter().enumerate() {
        if idx % 100 == 0 {
            println!("Processing card {}/{}", idx + 1, qty);
        }

        let card = provider.load(card).unwrap();
        let evs = card.into_events();

        if evs.is_empty() {
            continue;
        }

        if evs.len() == 1 {
            let event: LedgerEvent<RawCard> = evs.into_iter().next().unwrap().into();
            event_nodes.push(EventNode::new_leaf(event));
        } else {
            let events: Vec<ItemAction<RawCard>> = evs
                .into_iter()
                .map(|e| match e {
                    LedgerEvent::ItemAction { id, action } => ItemAction { id, action },
                    LedgerEvent::SetUpstream { .. } => panic!(),
                    LedgerEvent::DeleteSet { .. } => panic!(),
                })
                .collect();
            let entries = NonEmpty::from_vec(events).unwrap();
            event_nodes.push(EventNode::new_branch(entries));
        }
    }

    Ok(event_nodes)
}

pub fn save_event_nodes(
    event_nodes: Vec<EventNode<RawCard>>,
    path: &Path,
) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(&path)?;

    // Check if directory is not empty
    if path.read_dir()?.next().is_some() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "Directory already exists and is not empty",
        ));
    }

    // Save all event nodes
    if !event_nodes.is_empty() {
        let mut prev: Option<ledgerstore::LedgerEntry<RawCard>> = None;
        for (idx, node) in event_nodes.into_iter().enumerate() {
            let entry_node = node.save(&path, idx, prev.clone());
            prev = Some(entry_node.last().clone());
        }
    }

    Ok(())
}

#[derive(Clone)]
pub struct CardProvider {
    pub providers: Provider,
    time_provider: FsTime,
    pub recaller: ArcRecall,
    cache: Arc<RwLock<HashMap<CardId, Arc<Card>>>>,
}

impl Debug for CardProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CardProvider")
            .field("inner", &":)")
            .finish()
    }
}

impl CardProvider {
    pub fn load_all_card_ids(&self) -> Vec<CardId> {
        info!("x1");
        self.providers.cards.load_ids().into_iter().collect()
    }

    pub fn rewrite_ledger(&self, path: PathBuf) -> Result<(), std::io::Error> {
        let mut event_nodes: Vec<EventNode<RawCard>> = vec![];

        // Add SetUpstream as first node if commit and upstream are available from ledger
        let commit = self.providers.cards.current_commit();
        let upstream_url = self.providers.cards.current_upstream_url();

        if let (Some(commit), Some(upstream_url)) = (commit, upstream_url) {
            let upstream_event = LedgerEvent::SetUpstream {
                commit,
                upstream_url,
            };
            event_nodes.insert(0, EventNode::new_leaf(upstream_event));
        }

        save_event_nodes(event_nodes, &path)
    }

    pub fn load_metadata(&self, id: CardId) -> Option<Arc<Metadata>> {
        self.providers.metadata.load(id)
    }

    /// Finds cards whose display names are the same as another card's display name.
    pub fn duplicates(&self) -> IndexSet<String> {
        info!("finding duplicates!");
        let mut cards: Vec<String> = self
            .load_all()
            .into_iter()
            .map(|c| c.display_card().to_lowercase())
            .collect();

        cards.sort();

        let mut duplicates: IndexSet<String> = Default::default();

        let mut prev = String::new();
        for card in cards.into_iter() {
            if &card == &prev {
                duplicates.insert(card.clone());
            }

            prev = card;
        }

        duplicates
    }

    pub fn eval_dyncard(&self, dyncard: &DynCard) -> Vec<CardId> {
        match dyncard {
            DynCard::Instances(id) => {
                let sub_classes: ItemExpr<RawCard> = ItemExpr::Reference {
                    items: Box::new(ItemExpr::Item(*id)),
                    ty: Some(CardRefType::ParentClass),
                    reversed: true,
                    recursive: true,
                    include_self: true,
                };

                let expr = ItemExpr::Reference {
                    items: Box::new(sub_classes),
                    ty: Some(CardRefType::ClassOfInstance),
                    reversed: true,
                    recursive: true,
                    include_self: false,
                };

                self.providers.cards.load_expr(expr).into_iter().collect()
            }
            DynCard::CardType(ty) => self
                .providers
                .cards
                .get_prop_cache(PropertyCache::new(CardProperty::CardType, ty.to_string()))
                .into_iter()
                .collect(),

            DynCard::Dependents(id) => match self.load(*id) {
                Some(card) => card.direct_dependent_ids().into_iter().collect(),
                None => Default::default(),
            },

            DynCard::RecDependents(id) => {
                dbg!("rec dependents");
                match dbg!(self.load(*id)) {
                    Some(x) => x.recursive_dependents().into_iter().collect(),
                    None => return vec![],
                }
            }
        }
    }

    pub fn eval_expr(&self, expr: &SetExpr) -> IndexSet<CardId> {
        self.providers
            .cards
            .load_expr(expr.to_set().into())
            .into_iter()
            .collect()
    }

    pub fn modify_set(&self, event: SetEvent) -> Result<(), EventError<Set>> {
        self.providers.sets.modify(event)
    }

    fn load_set(&self, set: impl Into<ItemExpr<RawCard>>) -> IndexSet<CardId> {
        self.providers.cards.load_expr(set.into())
    }

    pub fn modify_metadata(&self, event: MetaEvent) -> Result<(), EventError<Metadata>> {
        let id = event.id();

        self.providers.metadata.modify(event)?;

        if let Some(id) = id {
            let mut guard = self.cache.write().unwrap();
            guard.remove(&id);

            for id in self.dependents(id) {
                guard.remove(&id);
            }
        }

        Ok(())
    }

    pub fn modify_review(&self, event: ReviewEvent) -> Result<(), EventError<History>> {
        let id = event.id();
        self.providers.reviews.modify(event)?;

        if let Some(id) = id {
            let mut guard = self.cache.write().unwrap();
            guard.remove(&id);

            /*
            for id in self.dependents(id) {
                guard.remove(&id);
            }
            */
        }

        Ok(())
    }

    pub fn delete_set(&self, set: ItemExpr<RawCard>) -> Result<(), EventError<RawCard>> {
        match self.many_modify(vec![Event::Card(CardEvent::DeleteSet { set })]) {
            Ok(()) => Ok(()),
            Err(MyEventError::CardError(e)) => Err(e),
            Err(_) => unreachable!(),
        }
    }

    pub fn many_modify(&self, events: Vec<Event>) -> Result<(), MyEventError> {
        let mut card_events: Vec<CardEvent> = vec![];
        let mut review_events: Vec<ReviewEvent> = vec![];
        let mut meta_events: Vec<MetaEvent> = vec![];

        for event in events {
            let card_ids: Vec<CardId> = match event {
                Event::Card(card_event) => {
                    let ids = match card_event.clone() {
                        LedgerEvent::ItemAction { id, .. } => vec![id],
                        LedgerEvent::SetUpstream { .. } => todo!(),
                        LedgerEvent::DeleteSet { set } => self.load_set(set).into_iter().collect(),
                    };

                    card_events.push(card_event);
                    ids
                }
                Event::History(review_event) => {
                    let id = review_event.id();
                    review_events.push(review_event);
                    id.into_iter().collect()
                }
                Event::Meta(meta_event) => {
                    let id = meta_event.id();
                    meta_events.push(meta_event);
                    id.into_iter().collect()
                }
            };

            for id in card_ids {
                let mut guard = self.cache.write().unwrap();
                guard.remove(&id);

                for id in self.dependents(id) {
                    guard.remove(&id);
                }
            }
        }

        if let Err(e) = self.providers.cards.modify_many(card_events) {
            return Err(MyEventError::CardError(e));
        }
        if let Err(e) = self.providers.reviews.modify_many(review_events) {
            return Err(MyEventError::ReviewError(e));
        }
        if let Err(e) = self.providers.metadata.modify_many(meta_events) {
            return Err(MyEventError::MetaError(e));
        }

        Ok(())
    }

    pub fn many_modify_card(&self, events: Vec<CardEvent>) -> Result<(), EventError<RawCard>> {
        for event in &events {
            if let Some(id) = event.id() {
                let mut guard = self.cache.write().unwrap();
                guard.remove(&id);

                for id in self.dependents(id) {
                    guard.remove(&id);
                }
            }
        }

        self.providers.cards.modify_many(events)
    }

    pub fn modify_card(&self, event: CardEvent) -> Result<(), EventError<RawCard>> {
        let id = event.id();

        self.providers.cards.modify(event)?;

        if let Some(id) = id {
            let mut guard = self.cache.write().unwrap();
            guard.remove(&id);

            for id in self.dependents(id) {
                guard.remove(&id);
            }
        }

        Ok(())
    }

    pub fn load_all(&self) -> Vec<Arc<Card>> {
        info!("load all");
        let mut out: Vec<Arc<Card>> = vec![];
        let ids = self.load_all_card_ids();

        for id in ids {
            let card = self.load(id).unwrap();
            self.cache.write().unwrap().insert(id, card.clone());
            out.push(card);
        }
        out
    }

    pub fn dependents(&self, id: CardId) -> IndexSet<CardId> {
        trace!("dependents of: {}", id);

        self.providers
            .cards
            .dependents_recursive(id)
            .into_iter()
            .collect()
    }

    pub fn load(&self, id: CardId) -> Option<Arc<Card>> {
        if let Some(card) = self.cache.read().unwrap().get(&id).cloned() {
            return Some(card);
        }

        let (base, is_remote) = self.providers.cards.load_with_remote_info(id)?;
        let history = match self.providers.reviews.load(id) {
            Some(revs) => revs,
            None => Arc::new(History::new(id)),
        };
        let metadata = match self.providers.metadata.load(id) {
            Some(meta) => meta,
            None => Arc::new(Metadata::new(id)),
        };

        let front_audio: Option<Audio> = None;
        let back_audio: Option<Audio> = None;

        let card = Arc::new(Card::from_parts(
            base,
            is_remote,
            history,
            metadata,
            self.clone(),
            self.recaller.clone(),
            front_audio,
            back_audio,
        ));

        self.cache.write().unwrap().insert(id, card.clone());

        Some(card)
    }

    pub fn time_provider(&self) -> FsTime {
        self.time_provider.clone()
    }

    /// Find a card by exact name match.
    /// Returns the best match using this priority:
    /// 1. Exact string match (case-sensitive)
    /// 2. Case-insensitive match
    /// 3. Normalized bigram match (handles punctuation/spacing differences)
    pub fn exact_match(&self, search_str: &str) -> Option<CardId> {
        use crate::card::{normalize_string, search_cards_by_text};

        let search_str = search_str.trim();
        let normalized_search = normalize_string(search_str);
        let candidate_cards: IndexSet<CardId> = self.providers.cards.load_expr(ItemExpr::All);

        let search_results = search_cards_by_text(
            &normalized_search,
            &candidate_cards,
            &self.providers.cards,
            10,
        );

        let exact_string = search_str.to_string();
        let lowercase = search_str.to_lowercase();

        // Find exact match - check card name with different matching levels
        let mut exact_match: Option<CardId> = None;
        let mut case_match: Option<CardId> = None;
        let mut bigram_match: Option<CardId> = None;

        for (_score, card_id) in search_results {
            if let Some(card) = self.load(card_id) {
                let name = card.name().to_string();

                if exact_string == name {
                    exact_match = Some(card_id);
                    break;
                } else if case_match.is_none() && lowercase == name.to_lowercase() {
                    case_match = Some(card_id);
                } else if bigram_match.is_none() && normalized_search == normalize_string(&name) {
                    bigram_match = Some(card_id);
                }
            }
        }

        exact_match.or(case_match).or(bigram_match)
    }

    pub fn new(provider: Provider, time_provider: FsTime, recaller: ArcRecall) -> Self {
        Self {
            time_provider,
            recaller,
            providers: provider,
            cache: Default::default(),
        }
    }
}
