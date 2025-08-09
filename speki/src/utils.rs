use std::{fmt::Debug, sync::Arc};

use clap::Parser;
use dioxus::prelude::*;
use ledgerstore::EventError;
use speki_core::{
    card::{CardId, RawCard},
    recall_rate::Recall,
    Card, CardRefType,
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
            dirs::data_local_dir().unwrap().join("speki")
        };

        Self(Arc::new(speki_core::App::new(root, !cli.remote)))
    }

    pub fn inner(&self) -> Arc<speki_core::App> {
        self.0.clone()
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
    let provider = APP.read().inner().card_provider.clone();

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
        EventError::Invariant(inv) => format!("invariant broken: {inv:?}"),
        EventError::Remote => format!("remote card cannot be modified"),
        EventError::ItemNotFound => format!("card not found"),
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
