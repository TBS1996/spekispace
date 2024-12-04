use dioxus::prelude::*;
use speki_core::{AnyType, Card};
use speki_dto::CardId;
use std::{cell::RefCell, sync::Arc};
use tracing::info;
use wasm_bindgen::prelude::*;

#[derive(Default, Clone, Debug)]
pub enum BrowsePage {
    #[default]
    Browse,
    View(Arc<Card<AnyType>>),
    SetDependency(Arc<Card<AnyType>>),
}

impl BrowsePage {
    pub fn get_card(&self) -> Option<Arc<Card<AnyType>>> {
        match self {
            BrowsePage::Browse => return None,
            BrowsePage::View(arc) => Some(arc.clone()),
            BrowsePage::SetDependency(arc) => Some(arc.clone()),
        }
    }
}

thread_local! {
    static SIGNAL: RefCell<Option<Signal<BrowsePage>>> = RefCell::new(None);
    static PROVIDER: RefCell<Option<Arc<speki_core::App>>> = RefCell::new(None);
    static FOOBAR: RefCell<Option<BrowsePage>> = RefCell::new(None);
}

pub fn set_browsepage(b: BrowsePage) {
    FOOBAR.with(|s| {
        *s.borrow_mut() = Some(b);
    });
}

pub fn take_browsepage() -> Option<BrowsePage> {
    FOOBAR.with(|s| s.borrow_mut().take())
}

pub fn set_app(app: Arc<speki_core::App>) {
    PROVIDER.with(|s| {
        *s.borrow_mut() = Some(app);
    });
}

pub fn set_signal(signal: Signal<BrowsePage>) {
    SIGNAL.with(|s| {
        *s.borrow_mut() = Some(signal);
    });
}

#[wasm_bindgen(js_name = onNodeClick)]
pub async fn on_node_click(node_id: &str) {
    info!("clicked node: {node_id}");
    let id = CardId(node_id.parse().unwrap());
    let provider = PROVIDER.with(|provider| provider.borrow().clone());
    if let Some(provider) = provider {
        let card = provider.load_card(id).await.unwrap();

        FOOBAR.with(|s| {
            let selected = BrowsePage::View(Arc::new(card.clone()));
            *s.borrow_mut() = Some(selected);
        });

        if let Some(mut sig) = SIGNAL.with(|signal| signal.borrow().clone()) {
            let selected = BrowsePage::View(Arc::new(card));
            info!("setting selected card: {selected:?}");
            sig.set(selected);
        }
    } else {
        tracing::warn!("Provider is not set.");
    }
}
