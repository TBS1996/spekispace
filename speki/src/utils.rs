use indexmap::IndexSet;
use std::{fmt::Debug, sync::Arc, time::Duration};

use clap::Parser;
use dioxus::prelude::*;
use ledgerstore::{EventError, ItemExpr, SavedItem};
use speki_core::{
    card::{CardError, CardId, RawCard},
    card_provider::CardProvider,
    collection::DynCard,
    ledger::{CardEvent, MetaEvent},
    metadata::Metadata,
    recall_rate::{History, Recall, ReviewEvent},
    set::{Set, SetEvent, SetExpr, SetId},
    Card, CardProperty, CardRefType, Config, MyEventError,
};

use crate::{overlays::OverlayEnum, Cli, APP};

#[derive(Clone)]
pub struct App(pub Arc<speki_core::App>);

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

    #[allow(dead_code)]
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

    pub fn all_dependents_with_ty(&self, key: CardId) -> IndexSet<(CardRefType, CardId)> {
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

    pub fn load_all_histories(&self) -> IndexSet<History> {
        self.0
            .provider
            .reviews
            .load_all()
            .into_iter()
            .map(SavedItem::into_inner)
            .collect()
    }

    pub fn dependencies_recursive(&self, key: CardId) -> IndexSet<CardId> {
        self.0.provider.cards.dependencies_recursive(key)
    }

    pub fn load_attrs(&self, id: CardId) -> IndexSet<CardId> {
        let expr = ItemExpr::Property {
            property: CardProperty::AttrId,
            value: id.to_string(),
        };

        self.0.provider.cards.load_expr(expr).into_iter().collect()
    }

    pub fn load_all_sets(&self) -> IndexSet<Set> {
        self.0
            .provider
            .sets
            .load_all()
            .into_iter()
            .map(SavedItem::into_inner)
            .collect()
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

    pub fn card_provider(&self) -> CardProvider {
        self.0.card_provider.clone()
    }

    pub fn eval_expr(&self, expr: &SetExpr) -> IndexSet<CardId> {
        self.0.card_provider.eval_expr(expr)
    }

    pub fn display_dyncard(&self, dyncard: &DynCard) -> String {
        dyncard.display(self.0.card_provider.clone())
    }

    pub fn duplicates(&self) -> IndexSet<String> {
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

pub fn handle_event_error(err: MyEventError) {
    match err {
        MyEventError::CardError(e) => handle_card_event_error(e),
        MyEventError::ReviewError(e) => handle_review_event_error(e),
        MyEventError::MetaError(e) => handle_meta_event_error(e),
    }
}

pub fn handle_card_event_error(err: EventError<RawCard>) {
    let provider = APP.read().0.card_provider.clone();
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
                let from = provider.load(from).unwrap().name().to_string();
                let to = provider.load(to).unwrap().name().to_string();
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
        EventError::Remote => format!("Remote card cannot be modified"),
        EventError::ItemNotFound(card) => format!("Card not found: {card}"),
        EventError::DeletingWithDependencies(_) => {
            format!("Cannot delete card: it has dependencies")
        }
        EventError::Invariant(card_error) => {
            let error_msg = match card_error {
                CardError::MissingParam { param_id } => {
                    let param_name = provider
                        .load(param_id)
                        .map(|c| c.name().to_string())
                        .unwrap_or_else(|| param_id.to_string());
                    format!("Missing required parameter: {param_name}")
                }
                CardError::InstanceOfNonClass { actual_type } => {
                    format!("Cannot create instance: card is of type {actual_type:?}, not a class")
                }
                CardError::AttributeOfNonInstance => {
                    format!("Cannot create attribute answer: card is not an instance")
                }
                CardError::MissingAttribute { attribute_id } => {
                    let attr_name = provider
                        .load(attribute_id)
                        .map(|c| c.name().to_string())
                        .unwrap_or_else(|| attribute_id.to_string());
                    format!("Missing required attribute: {attr_name}")
                }
                CardError::DefaultQuestionNotClass => {
                    format!("Default question can only be set on class cards")
                }
                CardError::WrongCardType { expected, actual } => {
                    format!("Wrong card type: expected {expected:?}, but got {actual:?}")
                }
                CardError::AnswerMustBeCard { attribute_id } => {
                    let attr_name = provider
                        .load(attribute_id)
                        .map(|c| c.name().to_string())
                        .unwrap_or_else(|| attribute_id.to_string());
                    format!("Attribute '{attr_name}' requires a card as answer, not text")
                }
                CardError::AnswerMustBeTime { attribute_id } => {
                    let attr_name = provider
                        .load(attribute_id)
                        .map(|c| c.name().to_string())
                        .unwrap_or_else(|| attribute_id.to_string());
                    format!("Attribute '{attr_name}' requires a timestamp as answer")
                }
                CardError::AnswerMustBeBool { attribute_id } => {
                    let attr_name = provider
                        .load(attribute_id)
                        .map(|c| c.name().to_string())
                        .unwrap_or_else(|| attribute_id.to_string());
                    format!("Attribute '{attr_name}' requires a boolean (true/false) as answer")
                }
                CardError::SubClassOfNonClass { parent_id } => {
                    let parent_name = provider
                        .load(parent_id)
                        .map(|c| c.name().to_string())
                        .unwrap_or_else(|| parent_id.to_string());
                    format!("Cannot set parent: '{parent_name}' is not a class")
                }
                CardError::BackTypeMustBeClass {
                    back_type_id,
                    actual_type,
                } => {
                    let back_name = provider
                        .load(back_type_id)
                        .map(|c| c.name().to_string())
                        .unwrap_or_else(|| back_type_id.to_string());
                    format!("Attribute back type must be a class: '{back_name}' is {actual_type:?}")
                }
                CardError::DuplicateAttribute { attribute_id } => {
                    let attr_name = provider
                        .load(attribute_id)
                        .map(|c| c.name().to_string())
                        .unwrap_or_else(|| attribute_id.to_string());
                    format!("Duplicate attribute: '{attr_name}' is already defined")
                }
                CardError::DuplicateParam { param_id } => {
                    let param_name = provider
                        .load(param_id)
                        .map(|c| c.name().to_string())
                        .unwrap_or_else(|| param_id.to_string());
                    format!("Duplicate parameter: '{param_name}' is already defined")
                }
                CardError::SimilarFront(card_id) => {
                    format!("Card front is too similar to existing card: {card_id}")
                }
            };
            error_msg
        }
    };

    OverlayEnum::new_notice(text).append();
}

pub fn handle_review_event_error(err: EventError<History>) {
    let text = match err {
        EventError::Cycle(items) => {
            unreachable!("reviews have dependencies: {items:?}")
        }
        EventError::Invariant(inv) => format!("invariant broken in review: {inv:?}"),
        EventError::Remote => format!("remote review cannot be modified"),
        EventError::ItemNotFound(card) => format!("review card not found: {card}"),
        EventError::DeletingWithDependencies(_) => {
            format!("cannot delete review with dependencies")
        }
    };

    OverlayEnum::new_notice(text).append();
}

pub fn handle_meta_event_error(err: EventError<Metadata>) {
    let text = match err {
        EventError::Cycle(items) => {
            let mut s = format!("cycle detected in metadata!\n");
            for (id, _) in items {
                s.push_str(&format!("Cycle involves: {id}\n"));
            }
            s
        }
        EventError::Invariant(inv) => format!("invariant broken in metadata: {inv:?}"),
        EventError::Remote => format!("remote metadata cannot be modified"),
        EventError::ItemNotFound(card) => format!("metadata not found: {card}"),
        EventError::DeletingWithDependencies(_) => {
            format!("cannot delete metadata with dependencies")
        }
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
