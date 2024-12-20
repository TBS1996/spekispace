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
use speki_web::{NodeMetadata, Origin};
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
}

impl Default for GraphRep {
    fn default() -> Self {
        Self::init(None)
    }
}

impl GraphRep {
    pub fn init(new_card_hook: Option<Arc<Box<dyn Fn(Arc<Card<AnyType>>)>>>) -> Self {
        let id = format!("cyto_id-{}", COUNTER.fetch_add(1, Ordering::SeqCst));
        Self {
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

    pub async fn set_origin(&self, origin: Origin) {
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

        let selv = self.clone();
        if let Some(whatever) = speki_web::take_graphaction(&self.cyto_id) {
            info!("nice clicked whatever!!! {whatever:?}");

            let app = APP.cloned();
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
                        let origin = selv.inner.origin().unwrap();
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
