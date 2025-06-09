use std::{
    fmt::Debug,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use digraph::RustGraph;
use dioxus::prelude::*;
use speki_core::{card::CardId, Card};
use speki_web::{Node, NodeMetadata};
use tracing::info;

use crate::{overlays::card_selector::MyClosure, utils, APP, ROUTE_CHANGE};

mod digraph;
#[cfg(feature = "web")]
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

#[derive(PartialEq, Clone)]
pub struct GraphRep {
    pub inner: RustGraph,
    pub is_init: Signal<bool>,
    pub cyto_id: Arc<String>,
    pub new_card_hook: Option<MyClosure>,
    pub label: Option<Signal<String>>,
    pub scope: Signal<usize>,
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
            is_init: Signal::new_in_scope(false, ScopeId::APP),
            cyto_id: Arc::new(id),
            new_card_hook: Default::default(),
            label: Default::default(),
            scope: Signal::new_in_scope(Default::default(), ScopeId::APP),
        }
    }

    pub fn with_hook(mut self, hook: MyClosure) -> Self {
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

    pub fn new_set_card_id(&self, card: CardId) {
        speki_web::set_graphaction(
            self.cyto_id.to_string(),
            speki_web::GraphAction::FromRust(Node::Card(card)),
        );
    }

    pub fn new_set_card(&self, card: Card) {
        self.new_set_card_id(card.id());
    }

    pub async fn clear(&self) {
        self.inner.clear();
        let rendered = self.is_dom_rendered();
        info!("rendered: {rendered}");

        if rendered {
            self.inner.create_cyto_graph(&self.cyto_id);
        }
    }

    fn is_dom_rendered(&self) -> bool {
        let x = utils::is_element_present(&self.cyto_id);
        x
    }
}

fn is_dom_rendered(cyto_id: &str) -> bool {
    let x = utils::is_element_present(cyto_id);
    x
}

#[component]
pub fn GraphRepRender(
    cyto_id: Arc<String>,
    scope: Signal<usize>,
    label: Option<Signal<String>>,
    inner: RustGraph,
    new_card_hook: Option<MyClosure>,
    is_init: Signal<bool>,
) -> Element {
    let cur_scope = current_scope_id().unwrap();
    scope.clone().set(cur_scope.0);
    info!("init scope: {scope:?}");
    speki_web::set_refresh_scope(cyto_id.to_string(), cur_scope);

    if let Some(label) = label.as_ref() {
        let label = label.cloned();
        if let Some(origin) = inner.origin() {
            #[cfg(feature = "web")]
            js::update_label(&cyto_id, origin, &label);
        } else {
            info!("no origin");
        }
    } else {
        info!("no label");
    }

    if let Some(whatever) = speki_web::take_graphaction(&cyto_id) {
        info!("nice clicked whatever!!! {whatever:?}");

        let app = APP.cloned();
        let inner = inner.clone();
        let cyto_id = cyto_id.clone();
        match whatever {
            speki_web::GraphAction::NodeClick(id) => {
                info!("node clicked!");

                spawn(async move {
                    let Some(id) = id.card_id() else {
                        return;
                    };

                    let card = app.load_card(id).await;

                    if let Some(hook) = new_card_hook.clone() {
                        hook.call(card.clone()).await;
                    }
                });
            }
            speki_web::GraphAction::FromRust(origin) => {
                spawn(async move {
                    inner.set_origin(origin).await;
                    if is_dom_rendered(&cyto_id) {
                        if let Some(label) = label {
                            let label = label.cloned();
                            inner.set_origin_label(&label);
                        }
                        inner.create_cyto_graph(&cyto_id);
                        if let Some(origin) = inner.origin() {
                            adjust_graph(&cyto_id, origin.id().to_string());
                        }
                        is_init.clone().set(true);
                    }
                });
            }
            speki_web::GraphAction::EdgeClick((from, to)) => {
                spawn(async move {
                    let origin = inner.origin().unwrap();
                    if origin.id() != from {
                        return;
                    };

                    match origin {
                        Node::Card(_) => {
                            let (Some(from), Some(to)) = (from.card_id(), to.card_id()) else {
                                return;
                            };
                            let mut first = app.load_card(from).await;
                            //first.write().rm_dependency(to).await;

                            inner.set_origin(Node::Card(from)).await;
                            if is_dom_rendered(&cyto_id) {
                                if let Some(label) = label {
                                    let label = label.cloned();
                                    inner.set_origin_label(&label);
                                }
                                inner.create_cyto_graph(&cyto_id);
                                if let Some(origin) = inner.origin() {
                                    adjust_graph(&cyto_id, origin.id().to_string());
                                }
                                is_init.clone().set(true);
                            }
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

                            inner
                                .set_origin(Node::Nope {
                                    node,
                                    dependencies,
                                    dependents,
                                })
                                .await;
                            if is_dom_rendered(&cyto_id) {
                                if let Some(label) = label {
                                    let label = label.cloned();
                                    inner.set_origin_label(&label);
                                }
                                inner.create_cyto_graph(&cyto_id);
                                if let Some(origin) = inner.origin() {
                                    adjust_graph(&cyto_id, origin.id().to_string());
                                }
                                is_init.clone().set(true);
                            }
                        }
                    }
                });
            }
        };
    } else {
        tracing::trace!("nope no set");
    }

    let cyto_id = cyto_id.clone();

    let rendered = is_dom_rendered(&cyto_id);
    tracing::trace!("rendered status: {rendered}");

    // We can't create the cyto instance until this function has been run at least once cause
    // cytoscape needs to connecto a valid DOM element, so it's a bit weird logic.
    // First time this function is run, it'll render an empty div, second time, the is_element_present will be
    // true and we create the instance, third time, is_init will be true and we won't trigger the create_instancea any longer.
    if !is_init.cloned() && rendered {
        let inner = inner.clone();
        let cyto_id = cyto_id.clone();
        spawn(async move {
            if is_dom_rendered(&cyto_id) {
                if let Some(label) = label {
                    let label = label.cloned();
                    inner.set_origin_label(&label);
                }
                inner.create_cyto_graph(&cyto_id);
                if let Some(origin) = inner.origin() {
                    adjust_graph(&cyto_id, origin.id().to_string());
                }
                is_init.clone().set(true);
            }
        });
    }

    if ROUTE_CHANGE.swap(false, Ordering::SeqCst) {
        info!("route change cause new cyto");
        let inner = inner.clone();
        let cyto_id = cyto_id.clone();
        spawn(async move {
            if is_dom_rendered(&cyto_id) {
                if let Some(label) = label {
                    let label = label.cloned();
                    inner.set_origin_label(&label);
                }
                inner.create_cyto_graph(&cyto_id);
                if let Some(origin) = inner.origin() {
                    adjust_graph(&cyto_id, origin.id().to_string());
                }
                is_init.clone().set(true);
            }
        });
    }

    rsx! {
        div {
            class: "flex flex-col grow w-full h-full",
            div {
                id: "{cyto_id}",
                class: "w-full h-full",

            }
        }
    }
}

fn adjust_graph(cyto_id: &str, origin: String) {
    info!("adjust graph");
    #[cfg(feature = "web")]
    js::run_layout(cyto_id, &origin);
    #[cfg(feature = "web")]
    js::zoom_to_node(cyto_id, &origin);
}
