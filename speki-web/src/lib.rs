use std::{cell::RefCell, collections::HashMap, sync::Arc};

use dioxus::prelude::*;
use speki_core::{AnyType, App, Card};
use speki_dto::{CType, CardId};
use tracing::info;
use uuid::Uuid;
use wasm_bindgen::prelude::*;

#[derive(Clone, Debug)]
pub enum GraphAction {
    NodeClick(NodeId),
    FromRust(Node),
    EdgeClick((NodeId, NodeId)),
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum NodeId {
    Temp(Uuid),
    Card(Uuid),
}

impl NodeId {
    pub fn card_id(&self) -> Option<CardId> {
        match self {
            NodeId::Temp(_) => None,
            NodeId::Card(uuid) => Some(CardId(*uuid)),
        }
    }

    pub fn new_from_card(id: CardId) -> Self {
        Self::Card(id.into_inner())
    }

    pub fn new_temp() -> Self {
        Self::Temp(Uuid::new_v4())
    }

    pub fn to_string(&self) -> String {
        match self {
            NodeId::Temp(uuid) => format!("TEMP-{}", uuid.to_string()),
            NodeId::Card(uuid) => uuid.to_string(),
        }
    }

    pub fn from_string(s: &str) -> Self {
        if let Some(end) = s.strip_prefix("TEMP-") {
            Self::Temp(end.parse().unwrap())
        } else {
            Self::Card(s.parse().unwrap())
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
    let id = NodeId::from_string(node_id);

    set_graphaction(cyto_id.to_string(), GraphAction::NodeClick(id));
    trigger_refresh(cyto_id);
}

#[wasm_bindgen(js_name = onEdgeClick)]
pub async fn on_edge_click(cyto_id: &str, source: &str, target: &str) {
    info!("okcyto id: {cyto_id}");
    info!("clicked node from {source} to {target}");

    let source = NodeId::from_string(source);
    let target = NodeId::from_string(target);

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeMetadata {
    pub id: NodeId,
    pub label: String,
    pub color: String,
    pub ty: CType,
    pub border: bool,
}

impl NodeMetadata {
    pub async fn from_card(card: Arc<Card<AnyType>>, is_origin: bool) -> Self {
        let label = card.print().await;
        let color = match card.recall_rate() {
            Some(rate) => rate_to_color(rate as f64 * 100.),
            None => cyan_color(),
        };

        let ty = card.card_type().fieldless();

        Self {
            id: NodeId::new_from_card(card.id),
            label,
            color,
            ty,
            border: is_origin,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Card(CardId),
    Nope {
        node: NodeMetadata,
        dependencies: Vec<Self>,
        dependents: Vec<Self>,
    },
}

impl Node {
    pub async fn non_recursive_dependents(&self, app: Arc<App>) -> Vec<NodeId> {
        match self {
            Node::Card(id) => app
                .load_card(*id)
                .await
                .unwrap()
                .dependents()
                .await
                .into_iter()
                .map(|card| NodeId::Card(card.id().into_inner()))
                .collect(),
            Node::Nope { dependents, .. } => dependents.into_iter().map(|dep| dep.id()).collect(),
        }
    }
    pub async fn non_recursive_dependencies(&self, app: Arc<App>) -> Vec<NodeId> {
        match self {
            Node::Card(id) => app
                .load_card(*id)
                .await
                .unwrap()
                .dependencies
                .into_iter()
                .map(|id| NodeId::Card(id.into_inner()))
                .collect(),
            Node::Nope { dependencies, .. } => {
                dependencies.into_iter().map(|dep| dep.id()).collect()
            }
        }
    }

    pub fn is_card(&self) -> bool {
        matches!(self, Self::Card(_))
    }
    pub fn id(&self) -> NodeId {
        match self {
            Node::Card(id) => NodeId::new_from_card(*id),
            Node::Nope { node, .. } => {
                info!("node: {node:?}");

                node.id.clone()
            }
        }
    }

    pub fn collect_dependents(&self) -> Vec<Self> {
        let mut result = Vec::new();
        if let Node::Nope { dependents, .. } = self {
            for dep in dependents {
                result.push(dep.clone());
                result.extend(dep.collect_dependents());
            }
        }
        result
    }

    pub fn collect_dependencies(&self) -> Vec<Self> {
        let mut result = Vec::new();
        if let Node::Nope { dependencies, .. } = self {
            for dep in dependencies {
                result.push(dep.clone());
                result.extend(dep.collect_dependencies());
            }
        }
        result
    }
}
