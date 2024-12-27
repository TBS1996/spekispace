use std::sync::Arc;

use dioxus::prelude::*;
use speki_core::{AnyType, Card, ClassCard, InstanceCard, NormalCard, UnfinishedCard};
use speki_dto::CardId;
use speki_web::{Node, NodeId, NodeMetadata};
use tracing::info;

use crate::{
    components::{BackPut, CardRef, CardTy, FrontPut, GraphRep, Komponent},
    overlays::{card_selector::CardSelector, Overlay},
    APP, OVERLAY,
};

#[derive(Clone)]
pub enum TempNode {
    Old(CardId),
    New {
        id: NodeId,
        front: FrontPut,
        dependencies: Signal<Vec<CardId>>,
        dependents: Signal<Vec<Node>>,
    },
}

impl From<TempNode> for Node {
    fn from(value: TempNode) -> Self {
        match value {
            TempNode::Old(card) => Self::Card(card),
            TempNode::New {
                id,
                front,
                dependencies,
                dependents,
            } => {
                let node = NodeMetadata {
                    id,
                    label: front.text.cloned(),
                    color: "#858585".to_string(),
                    ty: front.dropdown.selected.cloned().to_ctype(),
                    border: false,
                };

                let dependents = dependents.cloned();
                let dependencies: Vec<_> =
                    dependencies.cloned().into_iter().map(Node::Card).collect();

                Self::Nope {
                    node,
                    dependencies,
                    dependents,
                }
            }
        }
    }
}

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
    filter: Option<Arc<Box<dyn Fn(AnyType) -> bool>>>,
    tempnode: TempNode,
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

    pub fn with_filter(mut self, filter: Arc<Box<dyn Fn(AnyType) -> bool>>) -> Self {
        self.filter = Some(filter);
        self
    }

    pub fn with_dependents(mut self, deps: Vec<Node>) -> Self {
        self.dependents.extend(deps);
        self
    }

    pub async fn new_from_card(card: Arc<Card<AnyType>>, graph: GraphRep) -> Self {
        let tempnode = TempNode::Old(card.id);
        let filter = move |ty: AnyType| ty.is_class();

        let raw = card.to_raw();
        let concept = CardRef::new()
            .with_filter(Arc::new(Box::new(filter)))
            .with_dependents(tempnode.clone());
        if let Some(class) = raw.data.class().map(CardId) {
            let class = APP.read().load_card(class).await;
            concept.set_ref(class).await;
        }

        graph.new_set_card(card.clone());
        let dependencies = card.dependency_ids().await;
        let bck = BackPut::new(raw.data.back.clone()).with_dependents(tempnode.clone());
        let front = raw.data.front.unwrap_or_default();
        let back = raw.data.back.unwrap_or_default().to_string();
        let frnt = FrontPut::new(CardTy::from_ctype(card.get_ty().fieldless()));
        frnt.text.clone().set(front);

        bck.text.clone().set(back);

        let graph = graph.with_label(frnt.text.clone());

        Self {
            front: frnt,
            back: bck,
            dependencies: Signal::new_in_scope(dependencies.into_iter().collect(), ScopeId(3)),
            dependents: Signal::new_in_scope(Default::default(), ScopeId(3)),
            graph,
            is_done: Signal::new_in_scope(false, ScopeId(3)),
            old_card: Signal::new_in_scope(Some(card), ScopeId(3)),
            save_hook: None,
            title: None,
            filter: None,
            concept: concept.clone(),
            tempnode,
        }
    }

    pub fn new() -> Self {
        let front = FrontPut::new(CardTy::Normal);
        let dependencies = Signal::new_in_scope(Default::default(), ScopeId(3));
        let dependents = Signal::new_in_scope(Default::default(), ScopeId(3));

        let tempnode = TempNode::New {
            id: NodeId::new_temp(),
            front: front.clone(),
            dependencies: dependencies.clone(),
            dependents: dependents.clone(),
        };

        let back = BackPut::new(None).with_dependents(tempnode.clone());
        let filter = move |ty: AnyType| ty.is_class();
        let concept = CardRef::new()
            .with_filter(Arc::new(Box::new(filter)))
            .with_dependents(tempnode.clone());

        let label = front.text.clone();
        let selv = Self {
            concept,
            front,
            back,
            graph: GraphRep::default().with_label(label),
            is_done: Signal::new_in_scope(false, ScopeId(3)),
            old_card: Signal::new_in_scope(None, ScopeId(3)),
            save_hook: None,
            title: None,
            filter: None,
            dependencies,
            dependents,
            tempnode,
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

        if front.is_empty() {
            return None;
        }

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

    pub fn set_graph(&self) {
        let node = self.to_node();
        let dependencies = self.dependencies.cloned();
        self.graph
            .new_set_card_rep(node, dependencies, self.dependents.cloned());
    }

    fn to_node(&self) -> NodeMetadata {
        NodeMetadata {
            id: NodeId::new_temp(),
            label: self.front.text.cloned(),
            color: "#858585".to_string(),
            ty: self.front.dropdown.selected.cloned().to_ctype(),
            border: true,
        }
    }

    fn add_dep(&self) -> Element {
        let selv = self.clone();
        rsx! {
            button {
                class: "mt-6 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base md:mt-0",
                onclick: move |_| {
                    let selv = selv.clone();
                    let selv2 = selv.clone();

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

                    info!("1 scope is ! {:?}", current_scope_id().unwrap());

                    spawn(async move {
                        let dependent: Node = selv2.tempnode.clone().into();
                        let props = CardSelector::dependency_picker(Box::new(fun)).with_dependents(vec![dependent]);
                        OVERLAY.cloned().set(Box::new(props));
                        info!("2 scope is ! {:?}", current_scope_id().unwrap());

                    });
                },
                "add dependency"
            }
        }
    }

    fn save_button(&self) -> Element {
        let selv = self.clone();

        let enabled = selv.to_card().is_some_and(|card| {
            self.filter
                .as_ref()
                .map(|filter| (filter)(card.ty))
                .unwrap_or(true)
        });

        let class = if !enabled {
            "mt-6 inline-flex items-center text-white bg-gray-400 border-0 py-1 px-3 focus:outline-none cursor-not-allowed opacity-50 rounded text-base md:mt-0"
        } else {
            "mt-6 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base md:mt-0"
        };

        rsx! {
            button {
                class: "{class}",
                disabled: !enabled,
                onclick: move |_| {
                    if let Some(card) = selv.to_card() {
                        let selveste = selv.clone();
                        spawn(async move {
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

    fn input_elements(&self, ty: CardTy) -> Element {
        let selv = self.clone();
        rsx! {
            { self.front.render() }

            match ty {
                CardTy::Unfinished => rsx! {},

                CardTy::Normal => rsx! {
                    { selv.back.render() }
                },
                CardTy::Class => rsx! {
                    { selv.back.render() }
                    div {
                        class: "block text-gray-700 text-sm font-medium mb-2",
                        style: "margin-right: 81px;",
                        "Parent class"
                        { selv.concept.render() }
                    }
                },
                CardTy::Instance => rsx! {
                    { selv.back.render() }
                    div {
                        class: "block text-gray-700 text-sm font-medium mb-2",
                        style: "margin-right: 81px;",
                        "Class of instance"
                        { selv.concept.render() }
                    }
                },
            }

        }
    }

    fn render_inputs(&self) -> Element {
        info!("render inputs");
        let ty = self.front.dropdown.selected.clone();
        rsx! {
            { self.input_elements(ty.cloned()) }
            { self.add_dep() }
            { self.save_button() }
        }
    }
}

impl Overlay for CardViewer {
    fn is_done(&self) -> Signal<bool> {
        self.is_done.clone()
    }
}

impl Komponent for CardViewer {
    fn render(&self) -> Element {
        info!("render cardviewer");
        rsx! {
            div {
                class: "flex flex-col w-full h-full",
                if let Some(title) = self.title.as_ref() {
                    h1 {
                        class: "text-3xl font-bold mb-4 text-center",
                        "{title}"
                    }
                }

                div {
                    class: "flex flex-col md:flex-row w-full h-full overflow-hidden",
                    div {
                        class: "flex-none p-4 w-full max-w-[500px] box-border order-2 md:order-1 overflow-y-auto",
                        style: "min-height: 0; max-height: 100%;",
                        { self.render_inputs() }
                    }
                    div {
                        class: "flex-1 w-full box-border mb-2 md:mb-0 order-1 md:order-2",
                        style: "min-height: 0; flex-grow: 1;",
                        { self.graph.render() }
                    }
                }
            }
        }
    }
}
