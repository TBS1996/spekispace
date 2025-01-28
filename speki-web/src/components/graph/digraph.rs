use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use dioxus::prelude::*;
use petgraph::{
    algo::is_cyclic_directed,
    graph::{DiGraph, NodeIndex},
    visit::EdgeRef,
};
use speki_core::card::CType;
use speki_web::{Node, NodeId, NodeMetadata};
use tracing::info;

use crate::{utils::get_meta, App, APP};

impl PartialEq for RustGraph {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

#[derive(Clone, Default)]
pub struct RustGraph {
    inner: Arc<Mutex<DiGraph<NodeMetadata, ()>>>,
    origin: Arc<Mutex<Option<Node>>>,
    org_idx: Arc<Mutex<Option<NodeIndex>>>,
}

impl RustGraph {
    pub fn set_origin_label(&self, label: &str) {
        if let Some(orgidx) = self.org_idx.lock().unwrap().clone() {
            let mut guard = self.inner.lock().unwrap();
            let Some(node) = guard.node_weight_mut(orgidx) else {
                return;
            };
            node.label = label.to_string();
        }
    }

    async fn origin_and_graph(origin: Node) -> (Node, DiGraph<NodeMetadata, ()>, NodeIndex) {
        info!("XXX");
        let app = APP.cloned();
        let new_origin = match origin.clone() {
            Node::Card(id) => {
                let card = app.load_card(id).await;
                let mut dependents = vec![];
                let mut dependencies = vec![];

                for dep in card.dependencies() {
                    dependencies.push(Node::Card(dep));
                }

                for dep in card.card.read().dependents().await {
                    info!("adding dependent to origin: {dep:?}");
                    dependents.push(Node::Card(dep.id()));
                }

                let node = NodeMetadata::from_card(card, true).await;
                Node::Nope {
                    node,
                    dependencies,
                    dependents,
                }
            }
            x @ Node::Nope { .. } => x,
        };

        let (inner, idx) = new_inner_create_graph(app, new_origin).await;
        (origin, inner, idx)
    }

    pub async fn set_origin(&self, origin: Node) {
        let (origin, graph, idx) = Self::origin_and_graph(origin).await;
        *self.inner.lock().unwrap() = graph;
        *self.origin.lock().unwrap() = Some(origin);
        *self.org_idx.lock().unwrap() = Some(idx);
        self.transitive_reduction();
        assert!(!self.has_cycle());
    }

    fn transitive_reduction(&self) {
        use petgraph::{algo::has_path_connecting, visit::EdgeRef};
        let mut selv = self.inner.lock().unwrap();

        let all_edges: Vec<_> = selv
            .edge_references()
            .map(|edge| (edge.source(), edge.target()))
            .collect();

        for &(source, target) in &all_edges {
            let mut temp_graph = selv.clone();
            temp_graph.remove_edge(temp_graph.find_edge(source, target).unwrap());

            if has_path_connecting(&temp_graph, source, target, None) {
                let edge = selv.find_edge(source, target).unwrap();
                selv.remove_edge(edge);
            }
        }
    }

    fn has_cycle(&self) -> bool {
        is_cyclic_directed(&*self.inner.lock().unwrap())
    }

    pub fn origin(&self) -> Option<Node> {
        self.origin.lock().unwrap().clone()
    }

    pub fn create_cyto_graph(&self, cyto_id: &str) {
        info!("creating cyto instance");
        super::js::create_cyto_instance(cyto_id);
        tracing::trace!("adding nodes");
        let guard = self.inner.lock().unwrap();

        let mut nodes = vec![];

        for idx in guard.node_indices() {
            let node = &guard[idx];
            nodes.push(node.clone());
        }

        nodes.sort_by_key(|node| node.id.to_string());
        nodes.dedup_by_key(|node| node.id.to_string());

        for node in nodes.clone() {
            super::js::add_node(
                cyto_id,
                &node.id.to_string(),
                &node.label,
                &node.color,
                card_ty_to_shape(node.ty),
                node.border,
            );
        }

        tracing::trace!("adding edges");
        for edge in guard.edge_references() {
            let source = &guard[edge.source()];
            let target = &guard[edge.target()];

            super::js::add_edge(cyto_id, &source.id.to_string(), &target.id.to_string());
        }
    }

    pub fn clear(&self) {
        *self.inner.lock().unwrap() = Default::default();
        *self.origin.lock().unwrap() = Default::default();
    }
}

async fn collect_recursive_dependents(app: App, node: Node) -> Vec<Node> {
    let mut nodes = vec![];
    let mut foo = vec![];

    for node in node.collect_dependents() {
        nodes.push(node.clone());
        foo.push(node.clone());
    }

    for node in foo {
        match node {
            Node::Card(id) => {
                let card = app.load_card(id).await;
                let deps: Vec<Node> = card
                    .card
                    .read()
                    .all_dependents()
                    .await
                    .into_iter()
                    .map(Node::Card)
                    .collect();
                nodes.extend(deps);
            }
            Node::Nope { .. } => {}
        }
    }

    nodes.dedup();

    nodes
}

async fn collect_recursive_dependencies(app: App, node: Node) -> Vec<Node> {
    let mut nodes = vec![];
    let mut foo = vec![];

    for node in node.collect_dependencies() {
        nodes.push(node.clone());
        foo.push(node.clone());
    }

    for node in foo {
        match node {
            Node::Card(id) => {
                let card = app.load_card(id).await;
                let deps: Vec<Node> = card
                    .card
                    .read()
                    .all_dependencies()
                    .await
                    .into_iter()
                    .map(Node::Card)
                    .collect();
                nodes.extend(deps);
            }
            Node::Nope { .. } => {}
        }
    }

    nodes.dedup();

    nodes
}

async fn new_inner_create_graph(app: App, origin: Node) -> (DiGraph<NodeMetadata, ()>, NodeIndex) {
    info!("UU_______________________");
    info!("new inner origin: {origin:?}");

    let mut graph = DiGraph::new();
    let origin_index = graph.add_node(get_meta(&origin).await);
    let mut node_map: HashMap<NodeId, NodeIndex> = HashMap::new();
    node_map.insert(origin.id(), origin_index);
    let mut all_nodes = vec![origin.clone()];

    for dep in collect_recursive_dependents(app.clone(), origin.clone()).await {
        all_nodes.push(dep);
    }

    for dep in collect_recursive_dependencies(app.clone(), origin.clone()).await {
        all_nodes.push(dep);
    }

    all_nodes.sort_by_key(|node| node.id().to_string());
    all_nodes.dedup_by_key(|node| node.id().to_string());

    for node in all_nodes.clone() {
        let id = node.id();
        let meta = get_meta(&node).await;
        let node_idx = graph.add_node(meta);
        node_map.insert(id, node_idx);
    }

    let mut edges = vec![];

    for node in all_nodes.clone() {
        for dep in node.non_recursive_dependencies(app.inner()).await {
            if let Some(from) = node_map.get(&node.id()) {
                if let Some(to) = node_map.get(&dep) {
                    edges.push((*from, *to));
                }
            }
        }
    }

    for node in all_nodes {
        for dep in node.non_recursive_dependents(app.inner()).await {
            if let Some(to) = node_map.get(&node.id()) {
                if let Some(from) = node_map.get(&dep) {
                    edges.push((*from, *to));
                }
            }
        }
    }

    edges.sort();
    edges.dedup();

    for (from, to) in edges {
        graph.add_edge(from, to, ());
    }

    (graph, origin_index)
}

pub enum Shape {
    RoundedRectangle,
    Ellipse,
    Rectangle,
}

impl Shape {
    pub fn as_str(&self) -> &'static str {
        match self {
            Shape::RoundedRectangle => "roundrectangle",
            Shape::Ellipse => "ellipse",
            Shape::Rectangle => "rectangle",
        }
    }

    pub fn from_ctype(ty: CType) -> Self {
        match ty {
            CType::Instance => Self::RoundedRectangle,
            CType::Class => Self::Rectangle,
            CType::Unfinished => Self::Ellipse,
            CType::Attribute => Self::Ellipse,
            CType::Statement => Self::Ellipse,
            CType::Normal => Self::Ellipse,
            CType::Event => Self::Ellipse,
        }
    }
}

fn card_ty_to_shape(ty: CType) -> &'static str {
    Shape::from_ctype(ty).as_str()
}

/*
#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_meta(id: &str) -> NodeMetadata {
        NodeMetadata {
            id: id.to_string(),
            label: Default::default(),
            color: Default::default(),
            ty: CType::Normal,
        }
    }

    #[test]
    fn test_transitive_reduction_no_edges() {
        let mut graph = DiGraph::<NodeMetadata, ()>::new();

        // Graph with no nodes or edges
        transitive_reduction(&mut graph);

        // Assert that the graph remains empty
        assert_eq!(graph.node_count(), 0);
        assert_eq!(graph.edge_count(), 0);
    }

    #[test]
    fn test_transitive_reduction_simple() {
        let mut graph = DiGraph::<NodeMetadata, ()>::new();

        // Add nodes
        let a = graph.add_node(dummy_meta("A"));
        let b = graph.add_node(dummy_meta("B"));
        let c = graph.add_node(dummy_meta("C"));

        // Add edges
        graph.add_edge(a, b, ());
        graph.add_edge(b, c, ());
        graph.add_edge(a, c, ());

        // Before reduction
        assert_eq!(graph.edge_count(), 3);

        // Perform transitive reduction
        transitive_reduction(&mut graph);

        // After reduction
        assert_eq!(graph.edge_count(), 2);
        assert!(graph.find_edge(a, b).is_some());
        assert!(graph.find_edge(b, c).is_some());
        assert!(graph.find_edge(a, c).is_none());
    }

    #[test]
    fn test_transitive_reduction_no_reduction_needed() {
        let mut graph = DiGraph::<NodeMetadata, ()>::new();

        let a = graph.add_node(dummy_meta("A"));
        let b = graph.add_node(dummy_meta("B"));

        // Add a single edge
        graph.add_edge(a, b, ());

        // Perform transitive reduction
        transitive_reduction(&mut graph);

        // Assert no edges were removed
        assert_eq!(graph.edge_count(), 1);
        assert!(graph.find_edge(a, b).is_some());
    }

    #[test]
    fn test_transitive_reduction_complex() {
        let mut graph = DiGraph::<NodeMetadata, ()>::new();

        let a = graph.add_node(dummy_meta("A"));
        let b = graph.add_node(dummy_meta("B"));
        let c = graph.add_node(dummy_meta("C"));
        let d = graph.add_node(dummy_meta("D"));

        graph.add_edge(a, b, ());
        graph.add_edge(b, c, ());
        graph.add_edge(c, d, ());
        graph.add_edge(a, c, ());
        graph.add_edge(a, d, ());

        // Before reduction
        assert_eq!(graph.edge_count(), 5);

        // Perform transitive reduction
        transitive_reduction(&mut graph);

        // After reduction
        assert_eq!(graph.edge_count(), 3);
        assert!(graph.find_edge(a, b).is_some());
        assert!(graph.find_edge(b, c).is_some());
        assert!(graph.find_edge(c, d).is_some());
        assert!(graph.find_edge(a, c).is_none());
        assert!(graph.find_edge(a, d).is_none());
    }

    #[test]
    fn test_transitive_reduction_complex_large() {
        let mut graph = DiGraph::<NodeMetadata, ()>::new();

        // Add nodes
        let a = graph.add_node(dummy_meta("A"));
        let b = graph.add_node(dummy_meta("B"));
        let c = graph.add_node(dummy_meta("C"));
        let d = graph.add_node(dummy_meta("D"));
        let e = graph.add_node(dummy_meta("E"));
        let f = graph.add_node(dummy_meta("F"));
        let g = graph.add_node(dummy_meta("G"));
        let h = graph.add_node(dummy_meta("H"));

        // Add edges (with some redundant paths)
        graph.add_edge(a, b, ());
        graph.add_edge(b, c, ());
        graph.add_edge(c, d, ());
        graph.add_edge(a, c, ());
        graph.add_edge(a, d, ());
        graph.add_edge(b, d, ());
        graph.add_edge(d, e, ());
        graph.add_edge(e, f, ());
        graph.add_edge(c, f, ());
        graph.add_edge(a, f, ());
        graph.add_edge(f, g, ());
        graph.add_edge(d, g, ());
        graph.add_edge(e, g, ());
        graph.add_edge(g, h, ());

        // Before reduction
        assert_eq!(graph.edge_count(), 14);

        // Perform transitive reduction
        transitive_reduction(&mut graph); // or transitive_reduction(&mut graph);

        for edge in graph.edge_references() {
            let source = graph.node_weight(edge.source()).unwrap().id.clone();
            let target = graph.node_weight(edge.target()).unwrap().id.clone();
            dbg!((source, target));
        }

        // After reduction
        assert_eq!(graph.edge_count(), 7);

        // Check expected edges are present
        assert!(graph.find_edge(a, b).is_some());
        assert!(graph.find_edge(b, c).is_some());
        assert!(graph.find_edge(c, d).is_some());
        assert!(graph.find_edge(d, e).is_some());
        assert!(graph.find_edge(e, f).is_some());
        assert!(graph.find_edge(f, g).is_some());
        assert!(graph.find_edge(g, h).is_some());

        // Check redundant edges are removed
        assert!(graph.find_edge(a, c).is_none());
        assert!(graph.find_edge(a, d).is_none());
        assert!(graph.find_edge(a, f).is_none());
        assert!(graph.find_edge(b, d).is_none());
        assert!(graph.find_edge(c, f).is_none());
        assert!(graph.find_edge(d, g).is_none());
        assert!(graph.find_edge(e, g).is_none());
    }

    #[test]
    fn test_transitive_reduction_balfour() {
        let mut graph = DiGraph::<NodeMetadata, ()>::new();

        let why = graph.add_node(dummy_meta(
            "why was the publication of lord balfour's letter delayed?",
        ));
        let balfour = graph.add_node(dummy_meta("balfour declaration"));
        let palestine = graph.add_node(dummy_meta("Palestine"));
        let zionism = graph.add_node(dummy_meta("What is zionism?"));
        let public = graph.add_node(dummy_meta("public statement"));
        let jews = graph.add_node(dummy_meta("who are the jews?"));
        let judaism = graph.add_node(dummy_meta("what is judaism?"));
        let religion = graph.add_node(dummy_meta("religion"));
        let country = graph.add_node(dummy_meta("country 2"));
        let allencamp = graph.add_node(dummy_meta("allenbys campaign"));

        graph.add_edge(why, allencamp, ());
        graph.add_edge(why, balfour, ());

        graph.add_edge(balfour, palestine, ());
        graph.add_edge(balfour, zionism, ());
        graph.add_edge(balfour, public, ());
        graph.add_edge(balfour, jews, ());

        graph.add_edge(zionism, palestine, ());
        graph.add_edge(zionism, jews, ());

        graph.add_edge(jews, judaism, ());

        graph.add_edge(judaism, religion, ());

        graph.add_edge(palestine, country, ());

        assert_eq!(graph.edge_count(), 11);

        transitive_reduction(&mut graph);

        assert_eq!(graph.edge_count(), 9);

        // These are redunant because balfour depends on zionism which depends on palestine and jews
        assert!(graph.find_edge(balfour, palestine).is_none());
        assert!(graph.find_edge(balfour, jews).is_none());
    }
}
*/
