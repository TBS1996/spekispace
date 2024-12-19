use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fmt::Debug,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

use dioxus::prelude::*;
use petgraph::algo::is_cyclic_directed;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use speki_core::{AnyType, Card};
use speki_dto::{CType, CardId};
use speki_web::{NodeMetadata, Origin};
use tracing::info;
use web_sys::window;

use super::Komponent;
use crate::App;
use crate::{js, APP, ROUTE_CHANGE};

#[derive(Default)]
struct InnerGraph {
    graph: DiGraph<NodeMetadata, ()>,
    origin: Option<Origin>,
    _selected_edge: Option<petgraph::prelude::EdgeIndex>,
}

impl InnerGraph {
    pub async fn new(app: App, origin: Origin) -> Self {
        let mut graph = create_graph(app, origin.clone()).await;
        transitive_reduction(&mut graph);

        assert!(!is_cyclic_directed(&graph));

        Self {
            origin: Some(origin),
            graph,
            _selected_edge: None,
        }
    }
}

static COUNTER: AtomicUsize = AtomicUsize::new(0);

impl Debug for GraphRep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GraphRep")
            .field("app", &self.app)
            .field("is_init", &self.is_init)
            .field("cyto_id", &self.cyto_id)
            .finish()
    }
}

#[derive(Clone)]
pub struct GraphRep {
    app: App,
    inner: Arc<Mutex<InnerGraph>>,
    is_init: Arc<AtomicBool>,
    cyto_id: Arc<String>,
    new_card_hook: Option<Arc<Box<dyn Fn(Arc<Card<AnyType>>)>>>,
}

impl GraphRep {
    pub fn init(new_card_hook: Option<Arc<Box<dyn Fn(Arc<Card<AnyType>>)>>>) -> Self {
        let id = format!("cyto_id-{}", COUNTER.fetch_add(1, Ordering::SeqCst));
        let app = APP.cloned();
        Self {
            app,
            inner: Default::default(),
            is_init: Default::default(),
            cyto_id: Arc::new(id),
            new_card_hook,
        }
    }

    pub fn new_set_card_rep(&self, node: NodeMetadata, dependencies: Vec<CardId>) {
        let origin = Origin::Nope {
            node,
            dependencies,
            dependents: vec![],
        };
        speki_web::set_graphaction(
            self.cyto_id.to_string(),
            speki_web::GraphAction::FromRust(origin),
        );
    }

    pub fn new_set_card(&self, card: Arc<Card<AnyType>>) {
        speki_web::set_graphaction(
            self.cyto_id.to_string(),
            speki_web::GraphAction::FromRust(Origin::Card(card.id)),
        );
    }

    async fn set_card(&self, origin: Origin) {
        self.refresh(origin).await;
        self.create_cyto_instance().await;
    }

    pub async fn clear(&self) {
        {
            let new = InnerGraph::default();
            let mut inner = self.inner.lock().unwrap();
            *inner = new;
        }
        create_cyto_graph(&self.cyto_id, &Default::default());
    }

    async fn refresh(&self, origin: Origin) {
        let new = InnerGraph::new(self.app.clone(), origin).await;
        let mut inner = self.inner.lock().unwrap();
        *inner = new;
    }

    fn is_init(&self) -> bool {
        self.is_init.load(Ordering::SeqCst)
    }

    fn is_dom_rendered(&self) -> bool {
        is_element_present(&self.cyto_id)
    }

    async fn create_cyto_instance(&self) {
        let (graph, card) = {
            let guard = self.inner.lock().unwrap();
            let card = guard.origin.clone();
            let graph = guard.graph.clone();
            (graph, card)
        };

        let Some(card) = card else {
            return;
        };

        create_cyto_graph(&self.cyto_id, &graph);
        adjust_graph(&self.cyto_id, card.id().to_string());
        self.is_init.store(true, Ordering::SeqCst);
    }
}

impl Komponent for GraphRep {
    fn render(&self) -> Element {
        let scope = current_scope_id().unwrap();
        info!("init scope: {scope:?}");
        speki_web::set_refresh_scope(self.cyto_id.to_string(), scope);

        let selv = self.clone();
        if let Some(whatever) = speki_web::take_graphaction(&self.cyto_id) {
            info!("nice clicked whatever!!! {whatever:?}");

            let app = self.app.clone();
            match whatever {
                speki_web::GraphAction::NodeClick(id) => {
                    info!("node clicked!");

                    spawn(async move {
                        let Some(card) = app.0.load_card(id).await else {
                            return;
                        };

                        let card = Arc::new(card);
                        if let Some(hook) = selv.new_card_hook.as_ref() {
                            (hook)(card.clone());
                            selv.set_card(Origin::Card(id)).await;
                        }
                    });
                }
                speki_web::GraphAction::FromRust(card) => {
                    spawn(async move {
                        selv.set_card(card).await;
                    });
                }
                speki_web::GraphAction::EdgeClick((from, to)) => {
                    spawn(async move {
                        let origin = selv.inner.lock().unwrap().origin.clone().unwrap();
                        match origin {
                            Origin::Card(_) => {
                                let mut first = app.0.load_card(from).await.unwrap();
                                first.rm_dependency(to).await;
                                selv.set_card(Origin::Card(from)).await;
                            }
                            Origin::Nope {
                                node,
                                mut dependencies,
                                mut dependents,
                            } => {
                                let totlen = dependencies.len() + dependents.len();

                                dependencies.retain(|dep| dep != &to);
                                dependents.retain(|dep| dep != &to);

                                assert!(totlen != dependencies.len() + dependents.len());

                                selv.set_card(Origin::Nope {
                                    node,
                                    dependencies,
                                    dependents,
                                })
                                .await;
                            }
                        }
                    });
                }
            };
        } else {
            tracing::trace!("nope no set");
        }

        let cyto_id = self.cyto_id.clone();

        let rendered = self.is_dom_rendered();
        info!("rendered status: {rendered}");

        // We can't create the cyto instance until this function has been run at least once cause
        // cytoscape needs to connecto a valid DOM element, so it's a bit weird logic.
        // First time this function is run, it'll render an empty div, second time, the is_element_present will be
        // true and we create the instance, third time, is_init will be true and we won't trigger the create_instancea any longer.
        if !self.is_init() && rendered {
            let selv = self.clone();
            spawn(async move {
                selv.create_cyto_instance().await;
            });
        }

        if ROUTE_CHANGE.swap(false, Ordering::SeqCst) {
            info!("route change cause new cyto");
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

async fn create_graph(app: App, origin: Origin) -> DiGraph<NodeMetadata, ()> {
    let (origin, dependencies, dependents) = match origin {
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

    inner_create_graph(app, origin, dependencies, dependents).await
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

fn adjust_graph(cyto_id: &str, origin: String) {
    info!("adjust graph");
    js::run_layout(cyto_id, &origin);
    js::zoom_to_node(cyto_id, &origin);
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
