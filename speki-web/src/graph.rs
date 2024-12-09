use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use petgraph::algo::is_cyclic_directed;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use speki_core::{AnyType, Card};
use speki_dto::CType;
use tracing::info;
use web_sys::window;

use crate::js;
use crate::App;

use dioxus::prelude::*;

#[derive(Default)]
struct InnerGraph {
    graph: DiGraph<NodeMetadata, ()>,
    card: Option<Arc<Card<AnyType>>>,
    _selected_edge: Option<petgraph::prelude::EdgeIndex>,
}

impl InnerGraph {
    pub async fn new(app: App, card: Arc<Card<AnyType>>) -> Self {
        let card = (*card).clone().refresh().await;
        let mut graph = create_graph(app, card.clone()).await;
        transitive_reduction(&mut graph);

        assert!(!is_cyclic_directed(&graph));

        Self {
            card: Some(card),
            graph,
            _selected_edge: None,
        }
    }
}

#[derive(Clone)]
pub struct GraphRep {
    inner: Arc<Mutex<InnerGraph>>,
    /// Whether a cyto js instance has been created
    is_init: Arc<AtomicBool>,
    cyto_id: Arc<String>,
}

impl GraphRep {
    async fn refresh(&self, app: App, card: Arc<Card<AnyType>>) {
        let new = InnerGraph::new(app, card).await;
        let mut inner = self.inner.lock().unwrap();
        *inner = new;
    }

    fn is_init(&self) -> bool {
        self.is_init.load(Ordering::SeqCst)
    }

    fn is_dom_rendered(&self) -> bool {
        is_element_present(&self.cyto_id)
    }

    pub fn init(id: String) -> Self {
        Self {
            inner: Default::default(),
            is_init: Default::default(),
            cyto_id: Arc::new(id),
        }
    }

    pub async fn set_card(&self, app: App, card: Arc<Card<AnyType>>) {
        self.refresh(app, card).await;
        self.create_cyto_instance().await;
    }

    async fn create_cyto_instance(&self) {
        let (graph, card) = {
            let guard = self.inner.lock().unwrap();
            let card = guard.card.clone();
            let graph = guard.graph.clone();
            (graph, card)
        };

        let Some(card) = card else {
            return;
        };

        create_cyto_graph(&self.cyto_id, &graph);
        adjust_graph(&self.cyto_id, card.clone());
        self.is_init.store(true, Ordering::SeqCst);
    }

    pub fn render(&self) -> Element {
        let cyto_id = self.cyto_id.clone();

        // We can't create the cyto instance until this function has been run at least once cause
        // cytoscape needs to connecto a valid DOM element, so it's a bit weird logic.
        // First time this function is run, it'll render an empty div, second time, the is_element_present will be
        // true and we create the instance, third time, is_init will be true and we won't trigger the create_instancea any longer.
        if !self.is_init() && self.is_dom_rendered() {
            let selv = self.clone();
            spawn(async move {
                selv.create_cyto_instance().await;
            });
        }

        rsx! {
            div {
                id: "{cyto_id}",
                style: "width: 800px; height: 600px; border: 1px solid black;",
            }
        }
    }
}

fn is_element_present(id: &str) -> bool {
    window()
        .and_then(|win| win.document())
        .unwrap()
        .get_element_by_id(id)
        .is_some()
}

fn transitive_reduction(graph: &mut DiGraph<NodeMetadata, ()>) {
    use petgraph::algo::has_path_connecting;
    use petgraph::visit::EdgeRef;

    let all_edges: Vec<_> = graph
        .edge_references()
        .map(|edge| (edge.source(), edge.target()))
        .collect();

    for &(source, target) in &all_edges {
        let mut temp_graph = graph.clone();
        temp_graph.remove_edge(temp_graph.find_edge(source, target).unwrap());

        if has_path_connecting(&temp_graph, source, target, None) {
            graph.remove_edge(graph.find_edge(source, target).unwrap());
        }
    }
}

fn card_ty_to_shape(ty: CType) -> &'static str {
    match ty {
        CType::Instance => "roundrectangle",
        CType::Normal => "ellipse",
        CType::Unfinished => "ellipse",
        CType::Attribute => "ellipse",
        CType::Class => "rectangle",
        CType::Statement => "ellipse",
        CType::Event => "ellipse",
    }
}

#[derive(Clone, Debug)]
struct NodeMetadata {
    id: String,
    label: String,
    color: String,
    ty: CType,
}

fn create_cyto_graph(cyto_id: &str, graph: &DiGraph<NodeMetadata, ()>) {
    info!("creating cyto isntance");
    js::create_cyto_instance(cyto_id);
    info!("adding nodes");

    for idx in graph.node_indices() {
        let node = &graph[idx];
        js::add_node(
            cyto_id,
            &node.id,
            &node.label,
            &node.color,
            card_ty_to_shape(node.ty),
        );
    }

    info!("adding edges");
    for edge in graph.edge_references() {
        let source = &graph[edge.source()];
        let target = &graph[edge.target()];

        js::add_edge(cyto_id, &source.id, &target.id);
    }
}

fn adjust_graph(cyto_id: &str, card: Arc<Card<AnyType>>) {
    info!("adjust graph");
    let id = card.id.into_inner().to_string();
    js::run_layout(cyto_id, &id);
    js::zoom_to_node(cyto_id, &id);
}

async fn create_graph(app: App, card: Arc<Card<AnyType>>) -> DiGraph<NodeMetadata, ()> {
    info!("creating graph from card: {}", card.print().await);
    let mut graph = DiGraph::new();
    let mut node_map: HashMap<String, NodeIndex> = HashMap::new();

    let mut all_cards = Vec::new();

    for dependency in card.all_dependencies().await {
        if let Some(dep_card) = app.as_ref().load_card(dependency).await {
            all_cards.push(Arc::new(dep_card));
        }
    }

    for dependent in card.all_dependents().await {
        if let Some(dep_card) = app.as_ref().load_card(dependent).await {
            all_cards.push(Arc::new(dep_card));
        }
    }

    all_cards.push(card.clone());

    for card_ref in &all_cards {
        let id = card_ref.id.into_inner().to_string();
        if !node_map.contains_key(&id) {
            let label = card_ref.print().await;
            let color = if Arc::ptr_eq(&card, card_ref) {
                "#34ebdb".to_string()
            } else {
                "#32a852".to_string()
            };

            let ty = card_ref.card_type().fieldless();

            let metadata = NodeMetadata {
                id: id.clone(),
                label,
                color,
                ty,
            };

            let node_index = graph.add_node(metadata);
            node_map.insert(id, node_index);
        }
    }

    let mut edges = HashSet::<(NodeIndex, NodeIndex)>::default();

    for card_ref in &all_cards {
        let from_id = card_ref.id.into_inner().to_string();
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

    graph
}

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
