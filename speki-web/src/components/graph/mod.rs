use std::{
    fmt::Debug,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};

use digraph::RustGraph;
use dioxus::prelude::*;
use speki_core::{AnyType, Card};
use speki_dto::CardId;
use speki_web::{NodeMetadata, Node};
use tracing::info;
use web_sys::window;

use super::Komponent;
use crate::{APP, ROUTE_CHANGE};

mod digraph;
mod js;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

impl Debug for GraphRep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GraphRep")
            .field("is_init", &self.is_init)
            .field("cyto_id", &self.cyto_id)
            .finish()
    }
}

#[derive(Clone)]
pub struct GraphRep {
    inner: RustGraph,
    is_init: Arc<AtomicBool>,
    cyto_id: Arc<String>,
    new_card_hook: Option<Arc<Box<dyn Fn(Arc<Card<AnyType>>)>>>,
    label: Option<Signal<String>>,
}

impl Default for GraphRep {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphRep {
    pub fn new() -> Self {
        let id = format!("cyto_id-{}", COUNTER.fetch_add(1, Ordering::SeqCst));
        Self {
            inner: Default::default(),
            is_init: Default::default(),
            cyto_id: Arc::new(id),
            new_card_hook: Default::default(),
            label: Default::default(),
        }
    }

    pub fn with_hook(mut self, hook: Arc<Box<dyn Fn(Arc<Card<AnyType>>)>>) -> Self {
        self.new_card_hook = Some(hook);
        self
    }

    pub fn with_label(mut self, label: Signal<String>) -> Self {
        self.label = Some(label);
        self
    }

    pub fn new_set_card_rep(
        &self,
        node: NodeMetadata,
        dependencies: Vec<CardId>,
        dependents: Vec<Node>,
    ) {
        let origin = Node::Nope {
            node,
            dependencies: dependencies.into_iter().map(Node::Card).collect(),
            dependents,
        };
        speki_web::set_graphaction(
            self.cyto_id.to_string(),
            speki_web::GraphAction::FromRust(origin),
        );
    }

    pub fn new_set_card(&self, card: Arc<Card<AnyType>>) {
        speki_web::set_graphaction(
            self.cyto_id.to_string(),
            speki_web::GraphAction::FromRust(Node::Card(card.id)),
        );
    }

    async fn set_card(&self, origin: Node) {
        self.set_origin(origin).await;
        self.create_cyto_instance().await;
    }

    pub async fn clear(&self) {
        self.inner.clear();
        let rendered = self.is_dom_rendered();
        info!("rendered: {rendered}");

        if rendered {
            self.inner.create_cyto_graph(&self.cyto_id);
        }
    }

    pub async fn set_origin(&self, origin: Node) {
        self.inner.set_origin(origin).await;
    }

    pub async fn _refresh(&self) {
        match self.inner.origin() {
            Some(origin) => self.set_origin(origin).await,
            None => self.clear().await,
        }
    }

    fn is_init(&self) -> bool {
        self.is_init.load(Ordering::SeqCst)
    }

    fn is_dom_rendered(&self) -> bool {
        is_element_present(&self.cyto_id)
    }

    async fn create_cyto_instance(&self) {
        if self.is_dom_rendered() {
            if let Some(label) = self.label {
                let label = label.cloned();
                self.inner.set_origin_label(&label);
            }
            self.inner.create_cyto_graph(&self.cyto_id);
            if let Some(origin) = self.inner.origin() {
                adjust_graph(&self.cyto_id, origin.id().to_string());
            }
            self.is_init.store(true, Ordering::SeqCst);
        }
    }
}

impl Komponent for GraphRep {
    fn render(&self) -> Element {
        let scope = current_scope_id().unwrap();
        info!("init scope: {scope:?}");
        speki_web::set_refresh_scope(self.cyto_id.to_string(), scope);

        if let Some(label) = self.label.as_ref() {
            let label = label.cloned();
            if let Some(origin) = self.inner.origin() {
                js::update_label(&self.cyto_id, origin, &label);
            } else {
                info!("no origin");
            }
        } else {
            info!("no label");
        }

        let selv = self.clone();
        if let Some(whatever) = speki_web::take_graphaction(&self.cyto_id) {
            info!("nice clicked whatever!!! {whatever:?}");

            let app = APP.cloned();
            match whatever {
                speki_web::GraphAction::NodeClick(id) => {
                    info!("node clicked!");

                    spawn(async move {
                        let Some(id) = id.card_id() else {
                            return;
                        };

                        let card = app.load_card(id).await;

                        if let Some(hook) = selv.new_card_hook.as_ref() {
                            (hook)(card.clone());
                        }
                    });
                }
                speki_web::GraphAction::FromRust(origin) => {
                    spawn(async move {
                        selv.set_card(origin).await;
                    });
                }
                speki_web::GraphAction::EdgeClick((from, to)) => {
                    spawn(async move {
                        let origin = selv.inner.origin().unwrap();
                        if origin.id() != from {
                            return;
                        };

                        match origin {
                            Node::Card(_) => {
                                let (Some(from), Some(to)) = (from.card_id(), to.card_id()) else {
                                    return;
                                };
                                let mut first = Arc::unwrap_or_clone(app.load_card(from).await);
                                first.rm_dependency(to).await;
                                selv.set_card(Node::Card(from)).await;
                            }
                            Node::Nope {
                                node,
                                mut dependencies,
                                mut dependents,
                            } => {
                                let totlen = dependencies.len() + dependents.len();

                                dependencies.retain(|dep| dep.id() != to);
                                dependents.retain(|dep| dep.id() != to);

                                assert!(totlen != dependencies.len() + dependents.len());

                                selv.set_card(Node::Nope {
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
                class: "w-full h-full",

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

fn adjust_graph(cyto_id: &str, origin: String) {
    info!("adjust graph");
    js::run_layout(cyto_id, &origin);
    js::zoom_to_node(cyto_id, &origin);
}
