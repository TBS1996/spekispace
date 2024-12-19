use std::{
    collections::{BTreeSet, HashMap, HashSet},
    sync::{Arc, Mutex},
};

use dioxus::prelude::*;
use petgraph::{
    algo::is_cyclic_directed,
    graph::{DiGraph, NodeIndex},
    visit::EdgeRef,
};
use speki_dto::{CType, CardId};
use speki_web::{NodeMetadata, Origin};
use tracing::info;

use crate::{App, APP};

#[derive(Clone, Default)]
pub struct RustGraph {
    inner: Arc<Mutex<DiGraph<NodeMetadata, ()>>>,
    origin: Arc<Mutex<Option<Origin>>>,
}

impl RustGraph {
    async fn origin_and_graph(origin: Origin) -> (Origin, DiGraph<NodeMetadata, ()>) {
        let app = APP.cloned();
        let (org_node, dependencies, dependents) = match origin.clone() {
            Origin::Card(id) => {
                let card = app.0.load_card(id).await.unwrap();
                let mut dependents = vec![];
                let mut dependencies = vec![];

                for dep in card.dependency_ids().await {
                    dependencies.push(dep);
                }

                for dep in card.dependents().await {
                    dependents.push(dep.id);
                }

                let origin = NodeMetadata::from_card(&card, true).await;
                (origin, dependencies, dependents)
            }
            Origin::Nope {
                node,
                dependencies,
                dependents,
            } => (node, dependencies, dependents),
        };

        let inner = inner_create_graph(app, org_node.clone(), dependencies, dependents).await;
        (origin, inner)
    }

    pub async fn refresh(&self, origin: Origin) {
        let (origin, graph) = Self::origin_and_graph(origin).await;
        *self.inner.lock().unwrap() = graph;
        *self.origin.lock().unwrap() = Some(origin);
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

    pub fn origin(&self) -> Option<Origin> {
        self.origin.lock().unwrap().clone()
    }

    pub fn create_cyto_graph(&self, cyto_id: &str) {
        info!("creating cyto isntance");
        super::js::create_cyto_instance(cyto_id);
        info!("adding nodes");
        let guard = self.inner.lock().unwrap();

        for idx in guard.node_indices() {
            let node = &guard[idx];
            super::js::add_node(
                cyto_id,
                &node.id,
                &node.label,
                &node.color,
                card_ty_to_shape(node.ty),
            );
        }

        info!("adding edges");
        for edge in guard.edge_references() {
            let source = &guard[edge.source()];
            let target = &guard[edge.target()];

            super::js::add_edge(cyto_id, &source.id, &target.id);
        }
    }

    pub fn clear(&self) {
        *self.inner.lock().unwrap() = Default::default();
        *self.origin.lock().unwrap() = Default::default();
    }
}

async fn inner_create_graph(
    app: App,
    origin: NodeMetadata,
    dependencies: Vec<CardId>,
    dependents: Vec<CardId>,
) -> DiGraph<NodeMetadata, ()> {
    let mut graph = DiGraph::new();
    let origin_index = graph.add_node(origin.clone());
    let mut node_map: HashMap<String, NodeIndex> = HashMap::new();
    node_map.insert(origin.id.clone(), origin_index);

    let mut all_cards = BTreeSet::new();

    for dep in dependencies.clone() {
        let dep = Arc::new(app.0.load_card(dep).await.unwrap());
        all_cards.insert(dep.clone());
        for dep in dep.all_dependencies().await {
            let dep = app.0.load_card(dep).await.unwrap();
            all_cards.insert(Arc::new(dep));
        }
    }

    for dep in dependents.clone() {
        let dep = Arc::new(app.0.load_card(dep).await.unwrap());
        all_cards.insert(dep.clone());
        for dep in dep.all_dependents().await {
            let dep = app.0.load_card(dep).await.unwrap();
            all_cards.insert(Arc::new(dep));
        }
    }

    for card_ref in &all_cards {
        let id = card_ref.id.into_inner().to_string();
        if !node_map.contains_key(&id) {
            let node = NodeMetadata::from_card(card_ref, false).await;
            let node_index = graph.add_node(node);
            node_map.insert(id, node_index);
        }
    }

    let mut edges = HashSet::<(NodeIndex, NodeIndex)>::default();

    for card_ref in &all_cards {
        let from_id = card_ref.id.to_string();
        if let Some(&from_idx) = node_map.get(&from_id) {
            for dependency in card_ref.dependency_ids().await {
                let to_id = dependency.into_inner().to_string();
                if let Some(&to_idx) = node_map.get(&to_id) {
                    edges.insert((from_idx, to_idx));
                }
            }
        }
    }

    for (from_idx, to_idx) in edges {
        graph.add_edge(from_idx, to_idx, ());
    }

    for dep in dependencies {
        let from_idx = origin_index;
        let to_idx = *node_map.get(&dep.to_string()).unwrap();
        graph.add_edge(from_idx, to_idx, ());
    }

    for dep in dependents {
        let from_idx = *node_map.get(&dep.to_string()).unwrap();
        let to_idx = origin_index;
        graph.add_edge(from_idx, to_idx, ());
    }

    graph
}

enum Shape {
    RoundedRectangle,
    Ellipse,
    Rectangle,
}

impl Shape {
    fn as_str(&self) -> &'static str {
        match self {
            Shape::RoundedRectangle => "roundrectangle",
            Shape::Ellipse => "ellipse",
            Shape::Rectangle => "rectangle",
        }
    }

    fn from_ctype(ty: CType) -> Self {
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
