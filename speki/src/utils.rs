use std::{
    collections::{BTreeSet, HashSet},
    fmt::Debug,
    sync::Arc,
    time::Duration,
};

use clap::Parser;
use dioxus::prelude::*;
use ledgerstore::{EventError, PropertyCache, TheCacheGetter};
use speki_core::{
    card::{CardId, RawCard},
    card_provider::CardProvider,
    collection::{DynCard, MaybeCard},
    ledger::{CardEvent, MetaEvent},
    metadata::Metadata,
    recall_rate::{History, Recall, ReviewEvent},
    set::{Input, Set, SetEvent, SetExpr, SetId},
    Card, CardRefType, Config,
};

use crate::{overlays::OverlayEnum, Cli, APP};

#[derive(Clone)]
pub struct App(Arc<speki_core::App>);

impl App {
    pub fn new() -> Self {
        let cli = Cli::parse();

        let root = if cli.debug {
            let path = dirs::data_local_dir().unwrap().join("speki_debug");
            // creating a fresh root for debugging
            let _ = std::fs::remove_dir_all(&path);
            path
        } else if cli.debug_persist {
            dirs::data_local_dir().unwrap().join("speki_debug")
        } else if cli.remote {
            dirs::data_local_dir().unwrap().join("speki_remote")
        } else {
            Config::load().storage_path.clone()
        };

        Self(Arc::new(speki_core::App::new(root)))
    }

    pub fn modify_history(&self, event: ReviewEvent) -> Result<(), EventError<History>> {
        self.0.card_provider.modify_review(event)
    }

    pub fn modify_card(&self, event: CardEvent) -> Result<(), EventError<RawCard>> {
        self.0.card_provider.modify_card(event)
    }

    pub fn modify_meta(&self, event: MetaEvent) -> Result<(), EventError<Metadata>> {
        self.0.card_provider.modify_metadata(event)
    }

    pub fn modify_set(&self, event: SetEvent) -> Result<(), EventError<Set>> {
        self.0.card_provider.modify_set(event)
    }

    pub fn current_time(&self) -> Duration {
        use ledgerstore::TimeProvider;
        self.0.time_provider.current_time()
    }

    pub fn load(&self, id: CardId) -> Option<Arc<Card>> {
        self.0.card_provider.load(id)
    }

    pub fn all_dependents_with_ty(&self, key: CardId) -> HashSet<(CardRefType, CardId)> {
        self.0.provider.cards.all_dependents_with_ty(key)
    }

    pub fn load_set(&self, id: SetId) -> Option<Arc<Set>> {
        self.0.provider.sets.load(id)
    }

    pub fn load_metadata(&self, id: CardId) -> Option<Arc<Metadata>> {
        self.0.provider.metadata.load(id)
    }

    pub fn card_exists(&self, id: CardId) -> bool {
        self.0.provider.cards.has_item(id)
    }

    pub fn load_all_histories(&self) -> HashSet<History> {
        self.0.provider.reviews.load_all()
    }

    pub fn dependencies_recursive(&self, key: CardId) -> HashSet<CardId> {
        self.0.provider.cards.dependencies_recursive(key)
    }

    pub fn load_getter(&self, getter: TheCacheGetter<RawCard>) -> HashSet<CardId> {
        self.0.provider.cards.load_getter(getter)
    }

    pub fn load_all_sets(&self) -> HashSet<Set> {
        self.0.provider.sets.load_all()
    }

    pub fn card_ledger_hash(&self) -> Option<String> {
        self.0.provider.cards.currently_applied_ledger_hash()
    }
    pub fn meta_ledger_hash(&self) -> Option<String> {
        self.0.provider.metadata.currently_applied_ledger_hash()
    }

    pub fn set_ledger_hash(&self) -> Option<String> {
        self.0.provider.sets.currently_applied_ledger_hash()
    }

    pub fn get_prop_cache(&self, key: PropertyCache<RawCard>) -> HashSet<CardId> {
        self.0.provider.cards.get_prop_cache(key.clone())
    }

    pub fn card_provider(&self) -> CardProvider {
        self.0.card_provider.clone()
    }

    pub fn _eval_dyncard(&self, dyncard: &DynCard) -> Vec<MaybeCard> {
        self.0.card_provider.eval_dyncard(dyncard)
    }

    pub fn _eval_input(&self, input: &Input) -> BTreeSet<MaybeCard> {
        self.0.card_provider.eval_input(input)
    }

    pub fn eval_expr(&self, expr: &SetExpr) -> BTreeSet<MaybeCard> {
        self.0.card_provider.eval_expr(expr)
    }

    pub fn display_dyncard(&self, dyncard: &DynCard) -> String {
        dyncard.display(self.0.card_provider.clone())
    }

    pub fn duplicates(&self) -> HashSet<String> {
        self.0.card_provider.duplicates()
    }

    pub fn current_commit(&self) -> Option<String> {
        self.0.card_provider.providers.cards.current_commit()
    }

    pub fn latest_upstream_commit(&self) -> Option<String> {
        let curent_version = dbg!(speki_core::current_version());
        let config = Config::load();
        self.0.card_provider.providers.cards.latest_upstream_commit(
            curent_version,
            &config.remote_github_username,
            &config.remote_github_repo,
        )
    }

    pub fn try_load_card(&self, id: CardId) -> Option<Arc<Card>> {
        self.0.load_card(id).map(Arc::new)
    }
}

impl Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("App").field(&self.0).finish()
    }
}

pub fn handle_card_event_error(err: EventError<RawCard>) {
    let text = match err {
        EventError::Cycle(items) => {
            dbg!(&items);
            let mut foobar: Vec<(CardId, CardId, CardRefType)> = vec![];

            for i in 0..items.len() {
                let (to, ref_type) = &items[i];
                let from = if i == 0 {
                    items.last().unwrap().0.clone() // wraparound
                } else {
                    items[i - 1].0.clone()
                };
                foobar.push((from, to.clone(), ref_type.clone()));
            }

            let mut s = format!("cycle detected!\n");
            for (from, to, ty) in foobar {
                let from = APP.read().load(from).unwrap().name().to_string();
                let to = APP.read().load(to).unwrap().name().to_string();
                use speki_core::CardRefType as TY;

                let line = match ty {
                    TY::ExplicitDependency => format!("{from} depends on {to}"),
                    TY::ClassOfInstance => format!("{from} is an instance of {to} "),
                    TY::LinkRef => format!("{from} links to {to}"),
                    TY::ParentClass => format!("{from} is a parent class of {to}"),
                    TY::InstanceOfAttribute => format!("{from} is an instance of attribute {to}"),
                };
                s.push_str(&line);
                s.push_str("\n");
            }
            s
        }
        EventError::Invariant(inv) => format!("invariant broken: {inv:?}"),
        EventError::Remote => format!("remote card cannot be modified"),
        EventError::ItemNotFound(card) => format!("card not found: {card}"),
        EventError::DeletingWithDependencies => format!("cannot delete card with dependencies"),
    };

    OverlayEnum::new_notice(text).append();
}

pub fn recall_to_emoji(recall: Recall) -> &'static str {
    match recall {
        Recall::None => "😞",
        Recall::Late => "😐",
        Recall::Some => "🙂",
        Recall::Perfect => "😃",
    }
}
