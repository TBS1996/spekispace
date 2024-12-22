use std::sync::Arc;

use dioxus::prelude::*;
use speki_core::{AnyType, Card, ClassCard, InstanceCard, NormalCard, UnfinishedCard};
use speki_dto::CardId;
use speki_web::{Node, NodeId, NodeMetadata};
use tracing::info;

use crate::{
    components::{BackPut, CardRef, CardTy, FrontPut, GraphRep, Komponent},
    overlays::{card_selector::CardSelector, Overlay},
    APP, CARDS, OVERLAY,
};

pub struct CardRep {
    ty: AnyType,
    deps: Vec<CardId>,
}

#[derive(Clone)]
pub struct CardViewer {
    title: Option<String>,
    front: FrontPut,
    back: BackPut,
    concept: CardRef,
    dependencies: Signal<Vec<CardId>>,
    dependents: Signal<Vec<Node>>,
    graph: GraphRep,
    save_hook: Option<Arc<Box<dyn Fn(Arc<Card<AnyType>>)>>>,
    is_done: Signal<bool>,
    old_card: Signal<Option<Arc<Card<AnyType>>>>,
}

impl CardViewer {
    pub fn with_hook(mut self, hook: Arc<Box<dyn Fn(Arc<Card<AnyType>>)>>) -> Self {
        self.save_hook = Some(hook);
        self
    }

    pub fn with_title(mut self, title: String) -> Self {
        self.title = Some(title);
        self
    }

    pub fn with_dependents(mut self, deps: Vec<Node>) -> Self {
        self.dependents.extend(deps);
        self
    }

    pub async fn new_from_card(card: Arc<Card<AnyType>>, graph: GraphRep) -> Self {
        graph.new_set_card(card.clone());
        let entries = CARDS.cloned();
        let dependencies = card.dependency_ids().await;
        let raw = card.to_raw();
        let front = raw.data.front.unwrap_or_default();
        let back = raw.data.back.unwrap_or_default().to_string();
        let frnt = FrontPut::new();
        frnt.text.clone().set(front);
        let bck = BackPut::new();
        bck.text.clone().set(back);

        let graph = graph.with_label(frnt.text.clone());

        Self {
            front: frnt,
            back: bck,
            dependencies: Signal::new_in_scope(dependencies.into_iter().collect(), ScopeId(3)),
            dependents: Signal::new_in_scope(Default::default(), ScopeId(3)),
            graph,
            is_done: Signal::new_in_scope(false, ScopeId(3)),
            concept: CardRef::new(entries.classes.clone()),
            old_card: Signal::new_in_scope(Some(card), ScopeId(3)),
            save_hook: None,
            title: None,
        }
    }

    pub fn new() -> Self {
        let front = FrontPut::new();
        let label = front.text.clone();
        let selv = Self {
            front,
            back: BackPut::new(),
            dependencies: Signal::new_in_scope(Default::default(), ScopeId(3)),
            dependents: Signal::new_in_scope(Default::default(), ScopeId(3)),
            graph: GraphRep::default().with_label(label),
            is_done: Signal::new_in_scope(false, ScopeId(3)),
            concept: CardRef::new(CARDS.cloned().classes.clone()),
            old_card: Signal::new_in_scope(None, ScopeId(3)),
            save_hook: None,
            title: None,
        };

        selv.set_graph();
        selv
    }

    async fn reset(&self) {
        self.front.reset();
        self.back.reset();
        self.concept.reset();
        self.dependencies.clone().write().clear();
        self.old_card.clone().set(None);
        self.graph.clear().await;
    }

    fn to_card(&self) -> Option<CardRep> {
        let backside = self.back.clone();
        let frontside = self.front.clone();

        let front = format!("{}", frontside.text.cloned());
        let ty = match self.front.dropdown.selected.cloned() {
            CardTy::Normal => {
                let back = backside.to_backside()?;

                AnyType::Normal(NormalCard { front, back })
            }
            CardTy::Class => {
                let parent_class = self.concept.selected_card().cloned();
                let back = backside.to_backside()?;

                AnyType::Class(ClassCard {
                    name: front,
                    back,
                    parent_class,
                })
            }
            CardTy::Instance => {
                let class = self.concept.selected_card().cloned()?;
                let back = backside.to_backside();

                AnyType::Instance(InstanceCard {
                    name: front,
                    back,
                    class,
                })
            }
            CardTy::Unfinished => AnyType::Unfinished(UnfinishedCard { front }),
        };

        Some(CardRep {
            ty,
            deps: self.dependencies.cloned(),
        })
    }

    fn set_graph(&self) {
        let node = self.to_node();
        let dependencies = self.dependencies.cloned();
        self.graph
            .new_set_card_rep(node, dependencies, self.dependents.cloned());
    }

    fn to_node(&self) -> NodeMetadata {
        NodeMetadata {
            id: NodeId::new_temp(),
            label: self.front.text.cloned(),
            color: "#42cbf5".to_string(),
            ty: speki_dto::CType::Normal,
        }
    }

    fn render_inputs(&self) -> Element {
        let selv = self.clone();
        let selv2 = self.clone();
        let selv3 = self.clone();
        rsx! {

            { self.front.render() }

            match (selv.front.dropdown.selected)() {
                CardTy::Unfinished => {
                    rsx! {}
                }
                CardTy::Normal => {
                    rsx! {
                        { selv.back.render() }
                    }
                }
                CardTy::Class => {
                    rsx! {
                        { selv.back.render() }
                            div {
                                class: "block text-gray-700 text-sm font-medium mb-2",
                                "Parent class"
                                {selv.concept.render()},
                        }
                    }
                }
                CardTy::Instance => {
                    rsx! {
                        { selv.back.render() }

                        div {
                            class: "block text-gray-700 text-sm font-medium mb-2",
                            "Class of instance"
                            {selv.concept.render()},
                        }
                    }
                }
            }

            div {
                button {
                    class: "mt-6 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base md:mt-0",
                    onclick: move |_| {

                            let selv = selv3.clone();
                            let selfnode = selv.to_node();
                            info!("selfnode: {selfnode:?}");
                            let fun = move |card: Arc<Card<AnyType>>| {
                                selv.dependencies.clone().write().push(card.id);
                                selv.set_graph();
                                let old_card = selv.old_card.cloned();
                                spawn(async move {
                                    if let Some(old_card) = old_card {
                                        Arc::unwrap_or_clone(old_card).add_dependency(card.id).await;
                                    }
                                });
                            };


                            let viewer = Self::new()
                                .with_hook(Arc::new(Box::new(fun)))
                                .with_title("adding dependency".to_string())
                                .with_dependents(vec![Node::Nope { node: selfnode, dependencies: vec![], dependents: vec![] }]);
                            viewer.set_graph();
                            OVERLAY.cloned().set(Box::new(viewer));
                    },
                    "add new dependency"
                }
                button {
                    class: "mt-6 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base md:mt-0",
                    onclick: move |_| {
                            let selv = selv2.clone();

                            let fun = move |card: Arc<Card<AnyType>>| {
                                selv.dependencies.clone().write().push(card.id);
                                selv.set_graph();
                                let old_card = selv.old_card.cloned();
                                spawn(async move {
                                    if let Some(old_card) = old_card {
                                        Arc::unwrap_or_clone(old_card).add_dependency(card.id).await;
                                    }
                                });
                            };

                            let props = CardSelector::dependency_picker(Box::new(fun));

                            OVERLAY.cloned().set(Box::new(props));
                    },
                    "add existing dependency"
                }
                button {
                    class: "mt-6 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base md:mt-0",
                    onclick: move |_| {
                        if let Some(card) = selv.to_card() {
                            let selveste = selv.clone();
                            spawn(async move{
                                let mut new_raw = speki_core::card::new_raw_card(card.ty);

                                if let Some(card) = selveste.old_card.cloned() {
                                    new_raw.id = card.id.into_inner();
                                }

                                selv.is_done.clone().set(true);

                                for dep in card.deps {
                                    new_raw.dependencies.insert(dep.into_inner());
                                }

                                let card = APP.read().new_from_raw(new_raw).await;
                                if let Some(hook) = selveste.save_hook.clone() {
                                    (hook)(card);
                                }
                                selveste.reset().await;
                            });
                        }
                    },
                    "save"
                }
            }
        }
    }
}

impl Komponent for CardViewer {
    fn render(&self) -> Element {
        info!("_XX rendering display_card");

        let selv = self.clone();
        rsx! {
            div {
                class: "flex flex-col w-full h-[800px] mt-8",
                if let Some(title) = self.title.as_ref() {
                    h1 {
                        class: "text-3xl font-bold mb-4 text-center",
                        "{title}"
                    }
                }
                div {
                    class: "flex flex-row w-full h-full",
                    div {
                        class: "flex-[2] max-w-[600px] p-4 ml-20",
                        { selv.render_inputs() }
                    }
                    div {
                        class: "flex-[1] max-w-[700px] max-h-[700px]",
                        { self.graph.render() }
                    }
                }
            }
        }
    }
}

impl Overlay for CardViewer {
    fn is_done(&self) -> Signal<bool> {
        self.is_done.clone()
    }
}
