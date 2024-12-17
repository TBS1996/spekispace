use std::{cell::RefCell, collections::HashMap, sync::Arc};

use dioxus::prelude::*;
use speki_core::{AnyType, Card};
use speki_dto::CardId;
use tracing::info;
use wasm_bindgen::prelude::*;

#[derive(Default, Clone, Debug)]
pub enum BrowsePage {
    #[default]
    Browse,
    View(Arc<Card<AnyType>>),
}

#[derive(Clone, Debug)]
pub enum GraphAction {
    NodeClick(CardId),
    FromRust(Arc<Card<AnyType>>),
    EdgeClick((CardId, CardId)),
}

impl BrowsePage {
    pub fn get_card(&self) -> Option<Arc<Card<AnyType>>> {
        match self {
            BrowsePage::Browse => return None,
            BrowsePage::View(arc) => Some(arc.clone()),
        }
    }
}

thread_local! {
    static FOOBAR: RefCell<HashMap<String, Option<GraphAction>>> = RefCell::new(Default::default());
    static REFRESH_SCOPE: RefCell<HashMap<String, ScopeId>> = RefCell::new(Default::default());
}

pub fn set_graphaction(id: String, b: GraphAction) {
    FOOBAR.with(|s| {
        s.borrow_mut().insert(id, Some(b));
    });
}

pub fn take_graphaction(id: &str) -> Option<GraphAction> {
    FOOBAR.with(|s| s.borrow_mut().get_mut(id)?.take())
}

pub fn set_refresh_scope(id: String, signal: ScopeId) {
    REFRESH_SCOPE.with(|s| {
        s.borrow_mut().insert(id, signal);
    });
}

fn trigger_refresh(id: &str) {
    let scope = REFRESH_SCOPE.with(|provider| provider.borrow().get(id).unwrap().to_owned());
    info!("updating this scope: {scope:?}");
    scope.needs_update();
}

#[wasm_bindgen(js_name = onNodeClick)]
pub async fn on_node_click(cyto_id: &str, node_id: &str) {
    info!("cyto id: {cyto_id}");
    info!("!! clicked node: {node_id}");
    let id = CardId(node_id.parse().unwrap());

    set_graphaction(cyto_id.to_string(), GraphAction::NodeClick(id));
    trigger_refresh(cyto_id);
}

#[wasm_bindgen(js_name = onEdgeClick)]
pub async fn on_edge_click(cyto_id: &str, source: &str, target: &str) {
    info!("okcyto id: {cyto_id}");
    info!("clicked node from {source} to {target}");

    let source = CardId(source.parse().unwrap());
    let target = CardId(target.parse().unwrap());

    set_graphaction(
        cyto_id.to_string(),
        GraphAction::EdgeClick((source, target)),
    );
    trigger_refresh(cyto_id);
}
