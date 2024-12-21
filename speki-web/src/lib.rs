use std::{cell::RefCell, collections::HashMap, sync::Arc};

use dioxus::prelude::*;
use speki_core::{AnyType, Card};
use speki_dto::{CType, CardId};
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
    FromRust(Origin),
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
    static GRAPH_ACTIONS: RefCell<HashMap<String, Option<GraphAction>>> = RefCell::new(Default::default());
    static REFRESH_SCOPE: RefCell<HashMap<String, ScopeId>> = RefCell::new(Default::default());
}

pub fn set_graphaction(id: String, b: GraphAction) {
    GRAPH_ACTIONS.with(|s| {
        s.borrow_mut().insert(id, Some(b));
    });
}

pub fn take_graphaction(id: &str) -> Option<GraphAction> {
    GRAPH_ACTIONS.with(|s| s.borrow_mut().get_mut(id)?.take())
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

fn rate_to_color(rate: f64) -> String {
    let red = ((1.0 - rate / 100.0) * 255.0) as u8;
    let green = (rate / 100.0 * 255.0) as u8;
    format!("#{:02X}{:02X}00", red, green) // RGB color in hex
}

fn cyan_color() -> String {
    String::from("#00FFFF")
}

fn yellow_color() -> String {
    String::from("#FFFF00")
}

#[derive(Clone, Debug)]
pub struct NodeMetadata {
    pub id: String,
    pub label: String,
    pub color: String,
    pub ty: CType,
}

impl NodeMetadata {
    pub async fn from_card(card_ref: &Card<AnyType>, is_origin: bool) -> Self {
        let label = card_ref.print().await;
        let color = if is_origin {
            cyan_color()
        } else {
            match card_ref.recall_rate() {
                Some(rate) => rate_to_color(rate as f64 * 100.),
                None => yellow_color(),
            }
        };

        let ty = card_ref.card_type().fieldless();

        Self {
            id: card_ref.id.clone().to_string(),
            label,
            color,
            ty,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Origin {
    Card(CardId),
    Nope {
        node: NodeMetadata,
        dependencies: Vec<CardId>,
        dependents: Vec<CardId>,
    },
}

impl Origin {
    pub fn is_card(&self) -> bool {
        matches!(self, Self::Card(_))
    }
    pub fn id(&self) -> CardId {
        match self {
            Origin::Card(card_id) => *card_id,
            Origin::Nope { node, .. } => {
                info!("node: {node:?}");

                node.id.parse().unwrap()
            }
        }
    }
}
