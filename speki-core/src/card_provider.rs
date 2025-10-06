use crate::{
    audio::Audio,
    card::{CardId, RawCard},
    collection::DynCard,
    ledger::{CardEvent, MetaEvent},
    metadata::Metadata,
    recall_rate::{History, ReviewEvent},
    set::{Input, Set, SetEvent, SetExpr},
    ArcRecall, Card, CardProperty, CardRefType, FsTime, Provider,
};
use dioxus_logger::tracing::{info, trace};
use ledgerstore::{EventError, Leaf, PropertyCache, RefGetter};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fmt::Debug,
    sync::{Arc, RwLock},
};

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

    pub fn load_metadata(&self, id: CardId) -> Option<Arc<Metadata>> {
        self.providers.metadata.load(id)
    }

    pub fn duplicates(&self) -> HashSet<String> {
        info!("finding duplicates!");
        let mut cards: Vec<String> = self
            .load_all()
            .into_iter()
            .map(|c| c.display_card(true, true).to_lowercase())
            .collect();

        cards.sort();

        let mut duplicates: HashSet<String> = Default::default();

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
                let mut output = vec![];
                let getter = Leaf::Reference(RefGetter {
                    reversed: true,
                    key: *id,
                    ty: Some(CardRefType::ParentClass),
                    recursive: true,
                });
                let mut all_classes = dbg!(self.providers.cards.load_getter(getter));
                all_classes.insert(*id);

                for class in all_classes {
                    let getter = Leaf::Reference(RefGetter {
                        reversed: true,
                        key: class,
                        ty: Some(CardRefType::ClassOfInstance),
                        recursive: false,
                    });
                    for instance in self.providers.cards.load_getter(getter) {
                        output.push(instance);
                    }
                }

                output
            }
            DynCard::Trivial(flag) => self
                .providers
                .cards
                .get_prop_cache(PropertyCache::new(CardProperty::Trivial, flag.to_string()))
                .into_iter()
                .collect(),
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

    pub fn eval_input(&self, input: &Input) -> BTreeSet<CardId> {
        let res = match input {
            Input::Leaf(dc) => self.eval_dyncard(dc).into_iter().collect(),
            Input::Reference(id) => self.eval_expr(&self.providers.sets.load(*id).unwrap().expr),
            Input::Expr(expr) => self.eval_expr(&expr),
            Input::Card(id) => {
                let mut set = BTreeSet::default();
                set.insert(*id);
                set
            }
        };
        dbg!("evaluated: {:?}", self);
        res
    }

    pub fn eval_expr(&self, expr: &SetExpr) -> BTreeSet<CardId> {
        match expr {
            SetExpr::Union(hash_set) => {
                let mut out: BTreeSet<CardId> = Default::default();
                for input in hash_set {
                    out.extend(self.eval_input(input));
                }
                out
            }
            SetExpr::Intersection(hash_set) => {
                let mut iter = hash_set.into_iter();

                let Some(first) = iter.next() else {
                    return Default::default();
                };

                let mut set = self.eval_input(first);

                for input in iter {
                    set = set.intersection(&self.eval_input(input)).cloned().collect();
                }

                set
            }
            SetExpr::Difference(input1, input2) => {
                let set1 = self.eval_input(input1);
                let set2 = self.eval_input(input2);
                set1.difference(&set2).cloned().collect()
            }

            SetExpr::All => {
                self.eval_expr(&SetExpr::Complement(Input::Expr(Box::new(SetExpr::Union(
                    Default::default(),
                ))))) // complement of an empty union is the same as universe.
            }

            SetExpr::Complement(input) => self
                .providers
                .cards
                .load_ids()
                .difference(&self.eval_input(input).into_iter().collect())
                .cloned()
                .collect(),
        }
    }

    pub fn modify_set(&self, event: SetEvent) -> Result<(), EventError<Set>> {
        self.providers.sets.modify(event)
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

        let mut guard = self.cache.write().unwrap();

        for id in ids {
            let card = self.load(id).unwrap();
            guard.insert(id, card.clone());
            out.push(card);
        }
        out
    }

    pub fn dependents(&self, id: CardId) -> BTreeSet<CardId> {
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

    pub fn new(provider: Provider, time_provider: FsTime, recaller: ArcRecall) -> Self {
        Self {
            time_provider,
            recaller,
            providers: provider,
            cache: Default::default(),
        }
    }
}
