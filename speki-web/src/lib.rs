use dioxus::prelude::*;
use speki_core::{AnyType, Card};
use speki_dto::CardId;
use std::{cell::RefCell, sync::Arc};
use tracing::info;
use uuid::Uuid;
use wasm_bindgen::prelude::*;

pub fn say_hello() {
    println!("Hello from lib!");
}

thread_local! {
    static SIGNAL: RefCell<Option<Signal<Option<Arc<Card<AnyType>>>>>> = RefCell::new(None);
    static PROVIDER: RefCell<Option<Arc<speki_core::App>>> = RefCell::new(None);
}

pub fn set_app(app: Arc<speki_core::App>) {
    PROVIDER.with(|s| {
        *s.borrow_mut() = Some(app);
    });
}

pub fn set_signal(signal: Signal<Option<Arc<Card<AnyType>>>>) {
    SIGNAL.with(|s| {
        *s.borrow_mut() = Some(signal);
    });
}

#[wasm_bindgen(js_name = onNodeClick)]
pub async fn on_node_click(node_id: &str) {
    info!("clicked node: {node_id}");
    let id: Uuid = node_id.parse().unwrap();
    let id = CardId(id);
    info!("fethcing provider");
    let provider = PROVIDER.with(|provider| provider.borrow().clone());
    if let Some(provider) = provider {
        info!("lib loading card");
        let card = provider.load_card(id).await.unwrap();
        info!("lib cloning signal");
        if let Some(mut sig) = SIGNAL.with(|signal| signal.borrow().clone()) {
            info!("lib setting card");
            sig.set(Some(Arc::new(card)));
        }
    } else {
        info!("Provider is not set.");
    }
}
