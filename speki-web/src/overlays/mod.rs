use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use dioxus::prelude::*;
use tracing::info;

use crate::{components::Komponent, Route};

pub mod card_selector;
pub mod cardviewer;

pub trait Overlay: Komponent {
    fn is_done(&self) -> Signal<bool>;
}

#[derive(Clone, Default)]
pub struct OverlayManager {
    overlays: Arc<RwLock<HashMap<Route, Vec<Arc<Box<dyn Overlay>>>>>>,
    scopes: Arc<RwLock<HashMap<Route, ScopeId>>>,
}

impl OverlayManager {
    pub fn new() -> Self {
        Self::default()
    }

    fn update_scope(&self) {
        let route = use_route::<Route>();
        self.scopes
            .read()
            .unwrap()
            .get(&route)
            .unwrap()
            .needs_update();
    }

    pub fn replace(&self, popup: Box<dyn Overlay>) {
        info!("replace popup");
        let popup = Arc::new(popup);
        let route = use_route::<Route>();
        let mut guard = self.overlays.try_write().unwrap();
        let entry = guard.entry(route).or_default();
        entry.pop();
        entry.push(popup);
        self.update_scope();
    }

    pub fn set(&self, popup: Box<dyn Overlay>) {
        info!("set popup");
        let popup = Arc::new(popup);
        let route = use_route::<Route>();
        self.overlays
            .try_write()
            .unwrap()
            .entry(route)
            .or_default()
            .push(popup);
        self.update_scope();
    }

    pub fn render(&self) -> Option<Element> {
        info!("render popup!?");

        if let Ok(scope) = current_scope_id() {
            info!("overlay scope id: {scope:?}");
            self.set_scope(scope);
        }

        let pop = self.get_last_not_done()?;

        Some(rsx! {
        button {
            class: "float-right mr-4 mb-10",
            onclick: move |_| {
                pop.is_done().set(true);
            },
            "❌"
        },

        { pop.render() }
        })
    }

    fn get_last_not_done(&self) -> Option<Arc<Box<dyn Overlay>>> {
        let route = use_route::<Route>();
        loop {
            let last = self.overlays.read().unwrap().get(&route)?.last().cloned()?;

            if last.is_done().cloned() {
                self.overlays
                    .write()
                    .unwrap()
                    .get_mut(&route)
                    .unwrap()
                    .pop();
            } else {
                return Some(last);
            }
        }
    }

    fn set_scope(&self, scope: ScopeId) {
        let route = use_route::<Route>();
        self.scopes.write().unwrap().insert(route, scope);
    }
}
