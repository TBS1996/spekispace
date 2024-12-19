use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, RwLock,
};

use dioxus::prelude::*;
use tracing::info;

use crate::{components::Komponent, Route};

pub mod card_selector;
pub mod cardviewer;

pub trait PopTray: Komponent {
    fn is_done(&self) -> Signal<bool>;
}

#[derive(Clone, Default)]
pub struct PopupEntry {
    cards: Arc<RwLock<Vec<Arc<Popup>>>>,
    scope: Arc<AtomicUsize>,
}

pub type Popup = Box<dyn PopTray>;

#[derive(Clone, Default)]
pub struct OverlayManager {
    home: PopupEntry,
    review: PopupEntry,
    add: PopupEntry,
    browse: PopupEntry,
}

impl OverlayManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, popup: Popup) {
        info!("set popup");
        self.get().cards.write().unwrap().push(Arc::new(popup));
        self.get_scope().needs_update();
    }

    pub fn _replace(&self, popup: Popup) {
        info!("replace popup");
        let entry = self.get();
        let mut guard = entry.cards.write().unwrap();
        guard.pop();
        guard.push(Arc::new(popup));
    }

    pub fn render(&self) -> Option<Element> {
        info!("render popup!");

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

    fn get_last_not_done(&self) -> Option<Arc<Popup>> {
        loop {
            let last = self.get().cards.read().unwrap().last().cloned()?;
            if last.is_done().cloned() {
                self.get().cards.write().unwrap().pop();
            } else {
                return Some(last);
            }
        }
    }

    fn get(&self) -> PopupEntry {
        let route = use_route::<Route>();
        match route {
            Route::Home {} => self.home.clone(),
            Route::Review {} => self.review.clone(),
            Route::Add {} => self.add.clone(),
            Route::Browse {} => self.browse.clone(),
        }
    }

    fn set_scope(&self, scope: ScopeId) {
        let entry = self.get();
        entry.scope.store(scope.0, Ordering::SeqCst);
    }

    fn get_scope(&self) -> ScopeId {
        let scope = ScopeId(self.get().scope.load(Ordering::SeqCst));
        info!("got scope: {scope:?}");
        scope
    }
}
