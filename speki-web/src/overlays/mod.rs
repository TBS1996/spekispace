use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use dioxus::prelude::*;
use tracing::info;

use crate::{components::Komponent, Route, CURRENT_ROUTE, NONCLICKABLE};

pub mod card_selector;
pub mod cardviewer;
pub mod uploader;

pub trait Overlay: Komponent {
    fn is_done(&self) -> Signal<bool>;
}

#[derive(Clone, Default)]
pub struct OverlayManager {
    overlays: Arc<RwLock<HashMap<Route, Vec<Arc<Box<dyn Overlay>>>>>>,
    scopes: Arc<RwLock<HashMap<Route, ScopeId>>>,
}

impl OverlayManager {
    fn update_scope(&self) {
        let route = CURRENT_ROUTE.cloned();
        self.scopes
            .read()
            .unwrap()
            .get(&route)
            .unwrap()
            .needs_update();
    }

    pub fn replace(&self, popup: Box<dyn Overlay>) {
        info!("replace popup");
        self.pop();
        self.set(popup);
    }

    pub fn set(&self, popup: Box<dyn Overlay>) {
        info!("set popup");
        let popup = Arc::new(popup);
        info!("get route");
        let route = CURRENT_ROUTE.cloned();
        info!("route gotten!");
        self.overlays
            .try_write()
            .unwrap()
            .entry(route)
            .or_default()
            .push(popup);
        self.update_scope();
        let _x = NONCLICKABLE.cloned();
        NONCLICKABLE.read().clear();
    }

    pub fn render(&self) -> Option<Element> {
        info!("render popup!?");

        if let Ok(scope) = current_scope_id() {
            info!("overlay scope id: {scope:?}");
            self.set_scope(scope);
        }

        let pop = self.get_last_not_done()?;
        Some(rsx! {
            div {
                class: "h-full flex flex-col",
                div {
                    class: "h-8 bg-white flex items-center justify-end px-4 max-w-screen-xl mx-auto",
                    button {
                        class: "mr-4",
                        onclick: move |_| {
                            pop.is_done().set(true);
                        },
                        "❌"
                    }
                },
                div {
                    class: "flex-1 overflow-auto bg-white",
                    { pop.render() }
                }
            }
        })
    }

    pub fn pop(&self) -> Option<Arc<Box<dyn Overlay>>> {
        let route = CURRENT_ROUTE.cloned();
        let pop = self.overlays.write().unwrap().get_mut(&route)?.pop();
        self.update_scope();
        pop
    }

    fn last(&self) -> Option<Arc<Box<dyn Overlay>>> {
        let route = CURRENT_ROUTE.cloned();
        self.overlays.read().unwrap().get(&route)?.last().cloned()
    }

    fn get_last_not_done(&self) -> Option<Arc<Box<dyn Overlay>>> {
        loop {
            let last = self.last()?;

            if *last.is_done().read() {
                self.pop();
            } else {
                return Some(last);
            }
        }
    }

    fn set_scope(&self, scope: ScopeId) {
        let route = CURRENT_ROUTE.cloned();
        self.scopes.write().unwrap().insert(route, scope);
    }
}
