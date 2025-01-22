use std::sync::Arc;

use dioxus::prelude::*;
use speki_core::{
    card::{BaseCard, CardId},
    Card, CardType, ClassCard, InstanceCard, NormalCard, UnfinishedCard,
};
use speki_web::{Node, NodeId, NodeMetadata};
use tracing::info;

use crate::{
    components::{
        backside::BackPutRender, cardref::CardRefRender, frontside::FrontPutRender,
        graph::GraphRepRender, BackPut, CardRef, CardTy, DropDownMenu, FrontPut, GraphRep,
    },
    overlays::OverlayEnum,
    overlays::{
        card_selector::{CardSelector, MyClosure},
        yesno::Yesno,
    },
    APP, IS_SHORT,
};

#[derive(Clone, PartialEq)]
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

fn refresh_graph(
    graph: GraphRep,
    front: FrontPut,
    dependencies: Signal<Vec<CardId>>,
    dependents: Signal<Vec<Node>>,
    card: Option<NodeMetadata>,
) {
    let node = match card {
        Some(node) => node,
        None => NodeMetadata {
            id: NodeId::new_temp(),
            label: front.text.cloned(),
            color: "#858585".to_string(),
            ty: front.dropdown.selected.cloned().to_ctype(),
            border: true,
        },
    };

    graph.new_set_card_rep(node, dependencies.cloned(), dependents.cloned());
}

pub struct CardRep {
    ty: CardType,
    deps: Vec<CardId>,
}

#[derive(Props, Clone)]
pub struct CardViewer {
    pub title: Option<String>,
    pub front: FrontPut,
    pub back: BackPut,
    pub concept: CardRef,
    pub dependencies: Signal<Vec<CardId>>,
    pub dependents: Signal<Vec<Node>>,
    pub graph: GraphRep,
    pub save_hook: Option<Arc<Box<dyn Fn(Arc<Card>)>>>,
    pub is_done: Signal<bool>,
    pub old_card: Signal<Option<Arc<Card>>>,
    pub old_meta: Signal<Option<NodeMetadata>>,
    pub filter: Option<Callback<CardType, bool>>,
    pub tempnode: TempNode,
    pub allowed_cards: Vec<CardTy>,
    pub overlay: Signal<Option<OverlayEnum>>,
}

impl PartialEq for CardViewer {
    fn eq(&self, other: &Self) -> bool {
        self.title == other.title
            && self.front == other.front
            && self.back == other.back
            && self.concept == other.concept
            && self.dependencies == other.dependencies
            && self.dependents == other.dependents
            && self.graph == other.graph
            && self.is_done == other.is_done
            && self.old_card == other.old_card
            && self.old_meta == other.old_meta
            && self.filter == other.filter
            && self.tempnode == other.tempnode
            && self.allowed_cards == other.allowed_cards
            && self.overlay == other.overlay
    }
}

impl CardViewer {
    pub fn with_hook(mut self, hook: Arc<Box<dyn Fn(Arc<Card>)>>) -> Self {
        self.save_hook = Some(hook);
        self
    }

    pub fn with_front_text(self, text: String) -> Self {
        self.front.text.clone().set(text);
        self
    }

    pub fn with_title(mut self, title: String) -> Self {
        self.title = Some(title);
        self
    }

    pub fn with_filter(mut self, filter: Callback<CardType, bool>) -> Self {
        self.filter = Some(filter);
        self
    }

    pub fn with_allowed_cards(mut self, allowed: Vec<CardTy>) -> Self {
        if allowed.is_empty() {
            return self;
        }
        self.front.dropdown = DropDownMenu::new(allowed.clone(), None);
        self.allowed_cards = allowed;
        self
    }

    pub fn with_dependents(mut self, deps: Vec<Node>) -> Self {
        self.dependents.extend(deps);
        self
    }

    pub async fn new_from_card(card: Arc<Card>, graph: GraphRep) -> Self {
        let overlay: Signal<Option<OverlayEnum>> =
            Signal::new_in_scope(Default::default(), ScopeId::APP);
        let meta = NodeMetadata::from_card(card.clone(), true).await;

        let tempnode = TempNode::Old(card.id());
        let filter = Callback::new(move |ty: CardType| ty.is_class());

        let raw_ty = card.base.ty.clone();
        let concept = CardRef::new()
            .with_filter(filter)
            .with_dependents(tempnode.clone())
            .with_allowed(vec![CardTy::Class]);
        if let Some(class) = raw_ty.class() {
            let class = APP.read().load_card(class).await;
            concept.set_ref(class).await;
        }

        graph.new_set_card(card.clone());
        let dependencies = card.dependency_ids().await;
        let bck = BackPut::new(raw_ty.backside()).with_dependents(tempnode.clone());
        let front = raw_ty.raw_front();
        let back = raw_ty.raw_back();
        let frnt = FrontPut::new(CardTy::from_ctype(card.get_ty().fieldless()));
        frnt.text.clone().set(front);

        bck.text.clone().set(back);

        let graph = graph.with_label(frnt.text.clone());

        let dependencies: Signal<Vec<CardId>> =
            Signal::new_in_scope(dependencies.into_iter().collect(), ScopeId(3));
        let dependents: Signal<Vec<Node>> = Signal::new_in_scope(Default::default(), ScopeId(3));

        let _front = frnt.clone();
        let _graph = graph.clone();
        let _meta = meta.clone();
        let f = MyClosure(Arc::new(Box::new(move |card: Arc<Card>| {
            let graph = _graph.clone();
            let front = _front.clone();
            let meta = _meta.clone();
            let deps = dependencies.clone();
            deps.clone().write().push(card.id());
            refresh_graph(graph, front, deps, dependents.clone(), Some(meta));
        })));

        let _front = frnt.clone();
        let _graph = graph.clone();
        let _meta = meta.clone();
        let af = MyClosure(Arc::new(Box::new(move |card: Arc<Card>| {
            let graph = _graph.clone();
            let front = _front.clone();
            let deps = dependencies.clone();
            let meta = _meta.clone();
            deps.clone().write().retain(|dep| *dep != card.id());
            refresh_graph(graph, front, deps, dependents.clone(), Some(meta));
        })));

        let bck = bck.with_closure(f.clone()).with_deselect(af.clone());
        let concept = concept.with_closure(f.clone()).with_deselect(af.clone());

        Self {
            front: frnt,
            back: bck,
            dependents,
            dependencies,
            graph,
            is_done: Signal::new_in_scope(false, ScopeId(3)),
            old_card: Signal::new_in_scope(Some(card), ScopeId(3)),
            save_hook: None,
            title: None,
            filter: None,
            concept: concept.clone(),
            tempnode,
            allowed_cards: vec![],
            old_meta: Signal::new_in_scope(Some(meta), ScopeId::APP),
            overlay,
        }
    }

    pub fn new() -> Self {
        let overlay: Signal<Option<OverlayEnum>> =
            Signal::new_in_scope(Default::default(), ScopeId::APP);
        let front = FrontPut::new(CardTy::Normal);
        let dependencies: Signal<Vec<CardId>> =
            Signal::new_in_scope(Default::default(), ScopeId(3));
        let dependents = Signal::new_in_scope(Default::default(), ScopeId(3));
        let label = front.text.clone();
        let graph = GraphRep::default().with_label(label);
        let _graph = graph.clone();
        let _front = front.clone();

        let f = MyClosure(Arc::new(Box::new(move |card: Arc<Card>| {
            info!("ref card set ?");
            let graph = _graph.clone();
            let front = _front.clone();
            let deps = dependencies.clone();
            deps.clone().write().push(card.id());
            refresh_graph(graph, front, deps, dependents.clone(), None);
        })));

        let _front = front.clone();
        let _graph = graph.clone();
        let af = MyClosure(Arc::new(Box::new(move |card: Arc<Card>| {
            let graph = _graph.clone();
            let front = _front.clone();
            let deps = dependencies.clone();
            deps.clone().write().retain(|dep| *dep != card.id());
            refresh_graph(graph, front, deps, dependents.clone(), None);
        })));

        let tempnode = TempNode::New {
            id: NodeId::new_temp(),
            front: front.clone(),
            dependencies: dependencies.clone(),
            dependents: dependents.clone(),
        };

        let back = BackPut::new(None)
            .with_dependents(tempnode.clone())
            .with_closure(f.clone())
            .with_deselect(af.clone());

        let filter = Callback::new(move |ty: CardType| ty.is_class());

        let concept = CardRef::new()
            .with_filter(filter)
            .with_dependents(tempnode.clone())
            .with_allowed(vec![CardTy::Class])
            .with_closure(f.clone())
            .with_deselect(af.clone());

        let selv = Self {
            concept,
            front,
            back,
            graph,
            is_done: Signal::new_in_scope(false, ScopeId(3)),
            old_card: Signal::new_in_scope(None, ScopeId(3)),
            save_hook: None,
            title: None,
            filter: None,
            dependencies,
            dependents,
            tempnode,
            allowed_cards: vec![],
            old_meta: Signal::new_in_scope(None, ScopeId::APP),
            overlay,
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

                if back.is_empty_text() {
                    return None;
                }

                CardType::Normal(NormalCard { front, back })
            }
            CardTy::Class => {
                let parent_class = self.concept.selected_card().cloned();
                let back = backside.to_backside()?;

                CardType::Class(ClassCard {
                    name: front,
                    back,
                    parent_class,
                })
            }
            CardTy::Instance => {
                let class = self.concept.selected_card().cloned()?;
                let back = backside.to_backside();

                CardType::Instance(InstanceCard {
                    name: front,
                    back,
                    class,
                })
            }
            CardTy::Unfinished => CardType::Unfinished(UnfinishedCard { front }),
        };

        Some(CardRep {
            ty,
            deps: self.dependencies.cloned(),
        })
    }

    pub fn set_graph(&self) {
        if let Some(card) = self.old_card.cloned() {
            self.graph.new_set_card(card.clone());
            return;
        }

        refresh_graph(
            self.graph.clone(),
            self.front.clone(),
            self.dependencies.clone(),
            self.dependents.clone(),
            self.old_meta.cloned(),
        );
    }

    fn delete(&self, card: CardId) -> Element {
        let isdone = self.is_done.clone();
        let overlay = self.overlay.clone();
        rsx! {
            button {
                class: "mt-2 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base md:mt-0",
                onclick: move |_| {
                    let fun: Box<dyn Fn()> = Box::new(move || {
                        spawn(async move {
                            info!("cardviewer delete card! {card}");
                            APP.read().delete_card(card).await;
                            isdone.clone().set(true);
                        });
                    });

                    let yesno = Yesno::new("Really delete card?".to_string(), Arc::new(fun));
                    overlay.clone().set(Some(OverlayEnum::YesNo(yesno)));
                },
                "delete"
            }
        }
    }

    fn suspend(&self) -> Element {
        let Some(card) = self.old_card.cloned() else {
            return rsx! {};
        };

        let is_suspended = card.is_suspended();
        let txt = if is_suspended { "unsuspend" } else { "suspend" };
        let selv = self.clone();

        rsx! {
            button {
                class: "mt-2 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base md:mt-0",
                onclick: move |_| {
                    let card = card.clone();
                    let selv = selv.clone();
                    spawn(async move {
                        let mut card = Arc::unwrap_or_clone(card);
                        card.set_suspend(!is_suspended).await;
                        selv.old_card.clone().set(Some(Arc::new(card)));
                    });
                },
                "{txt}"
            }
        }
    }

    fn add_dep(&self) -> Element {
        let selv = self.clone();
        let overlay = self.overlay.clone();
        rsx! {
            button {
                class: "mt-2 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base md:mt-0",
                onclick: move |_| {
                    let selv = selv.clone();
                    let selv2 = selv.clone();

                    let fun = MyClosure(Arc::new(Box::new(
                        move |card: Arc<Card>| {
                        selv.dependencies.clone().write().push(card.id());
                        selv.set_graph();
                        let old_card = selv.old_card.cloned();
                        spawn(async move {
                            if let Some(old_card) = old_card {
                                Arc::unwrap_or_clone(old_card).add_dependency(card.id()).await;
                            }
                        });
                    }


                )));

                    info!("1 scope is ! {:?}", current_scope_id().unwrap());

                    spawn(async move {
                        let dependent: Node = selv2.tempnode.clone().into();
                        let props = CardSelector::dependency_picker(fun).await.with_dependents(vec![dependent]);
                        overlay.clone().set(Some(OverlayEnum::CardSelector(props)));
                        info!("2 scope is ! {:?}", current_scope_id().unwrap());
                    });
                },
                "add dependency"
            }
        }
    }

    fn save_button(&self) -> Element {
        let selv = self.clone();

        let is_new = self.old_card.as_ref().is_none();

        let enabled = selv.to_card().is_some_and(|card| {
            self.filter
                .as_ref()
                .map(|filter| (filter)(card.ty))
                .unwrap_or(true)
        });

        let class = if !enabled {
            "mt-2 inline-flex items-center text-white bg-gray-400 border-0 py-1 px-3 focus:outline-none cursor-not-allowed opacity-50 rounded text-base md:mt-0"
        } else {
            "mt-2 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base md:mt-0"
        };

        rsx! {
            button {
                class: "{class}",
                disabled: !enabled,
                onclick: move |_| {
                    if let Some(card) = selv.to_card() {
                        let selveste = selv.clone();
                        spawn(async move {

                            let id = selveste.old_card.cloned().map(|card|card.id());
                            let mut basecard = BaseCard::new_with_id(id, card.ty);

                            selv.is_done.clone().set(true);

                            for dep in card.deps {
                                basecard.dependencies.insert(dep);
                            }

                            let card = APP.read().inner().card_provider().save_basecard(basecard).await;
                            if let Some(hook) = selveste.save_hook.clone() {
                                (hook)(card);
                            }
                            selveste.reset().await;
                        });
                    }
                },
                if is_new {
                    "create"
                } else {
                    "save"
                }
            }
        }
    }

    fn input_elements(&self, ty: CardTy) -> Element {
        let selv = self.clone();
        let is_short = IS_SHORT.cloned();
        let overlay = self.overlay.clone();
        rsx! {
            FrontPutRender { dropdown: self.front.dropdown.clone(), text: self.front.text.clone() }

            match ty {
                CardTy::Unfinished => rsx! {},

                CardTy::Normal => rsx! {
                    BackPutRender {
                        text: selv.back.text.clone(),
                        dropdown: selv.back.dropdown.clone(),
                        ref_card: selv.back.ref_card.clone(),
                        overlay: overlay.clone(),
                    }
                },
                CardTy::Class => rsx! {
                    BackPutRender {
                        text: selv.back.text.clone(),
                        dropdown: selv.back.dropdown.clone(),
                        ref_card: selv.back.ref_card.clone(),
                        overlay: overlay.clone(),
                    }


                    if !is_short {
                        div {
                            class: "block text-gray-700 text-sm font-medium mb-2",
                            style: "margin-right: 81px;",
                            "Parent class"

                            CardRefRender{
                                card_display: selv.concept.display.clone(),
                                selected_card: selv.concept.card.clone(),
                                placeholder: "pick parent class",
                                on_select: selv.concept.on_select.clone(),
                                on_deselect: selv.concept.on_deselect.clone(),
                                dependent: selv.concept.dependent.clone(),
                                filter: selv.concept.filter.clone(),
                                allowed: selv.concept.allowed.clone(),
                                overlay: overlay.clone(),
                            },
                        }
                    } else {
                        div {
                            class: "block text-gray-700 text-sm font-medium",
                            style: "margin-right: 81px;",

                            CardRefRender{
                                card_display: selv.concept.display.clone(),
                                selected_card: selv.concept.card.clone(),
                                placeholder: "pick parent class",
                                on_select: selv.concept.on_select.clone(),
                                on_deselect: selv.concept.on_deselect.clone(),
                                dependent: selv.concept.dependent.clone(),
                                filter: selv.concept.filter.clone(),
                                allowed: selv.concept.allowed.clone(),
                        overlay: overlay.clone(),
                            },
                        }
                    }
                },
                CardTy::Instance => rsx! {
                    BackPutRender {
                        text: selv.back.text.clone(),
                        dropdown: selv.back.dropdown.clone(),
                        ref_card: selv.back.ref_card.clone(),
                        overlay: overlay.clone(),
                    }

                    if !is_short {
                        div {
                            class: "block text-gray-700 text-sm font-medium mb-2",
                            style: "margin-right: 81px;",
                            "Class of instance"
                            CardRefRender{
                                card_display: selv.concept.display.clone(),
                                selected_card: selv.concept.card.clone(),
                                placeholder: "pick class of instance",
                                on_select: selv.concept.on_select.clone(),
                                on_deselect: selv.concept.on_deselect.clone(),
                                dependent: selv.concept.dependent.clone(),
                                filter: selv.concept.filter.clone(),
                                allowed: selv.concept.allowed.clone(),
                                overlay: overlay.clone(),
                            },
                        }
                    } else {
                        div {
                            class: "block text-gray-700 text-sm font-medium",
                            style: "margin-right: 81px;",
                            CardRefRender{
                                card_display: selv.concept.display.clone(),
                                selected_card: selv.concept.card.clone(),
                                placeholder: "pick class of instance",
                                on_select: selv.concept.on_select.clone(),
                                on_deselect: selv.concept.on_deselect.clone(),
                                dependent: selv.concept.dependent.clone(),
                                filter: selv.concept.filter.clone(),
                                allowed: selv.concept.allowed.clone(),
                                overlay: overlay.clone(),
                            },
                        }
                    }
                },
            }
        }
    }

    fn render_inputs(&self) -> Element {
        info!("render inputs");
        let ty = self.front.dropdown.selected.clone();
        rsx! {
            div {
                { self.input_elements(ty.cloned()) }
            }
            div {
                if let Some(card) = self.old_card.cloned() {
                    {self.delete(card.id())}
                    {self.suspend()}
                }
                { self.add_dep() }
                { self.save_button() }
            }
        }
    }
}

#[component]
pub fn CardViewerRender(props: CardViewer) -> Element {
    info!("render cardviewer");

    rsx! {
        div {
            class: "flex flex-col w-full h-full",
            if let Some(title) = props.title.as_ref() {
                h1 {
                    class: "text-3xl font-bold mb-4 text-center",
                    "{title}"
                }
            }

            div {
                class: "flex flex-col md:flex-row w-full h-full overflow-hidden",
                div {
                    class: "flex-none p-2 w-full max-w-[500px] box-border order-2 md:order-1 overflow-y-auto",
                    style: "min-height: 0; max-height: 100%;",
                    { props.render_inputs() }
                }
                div {
                    class: "flex-1 w-full box-border mb-2 md:mb-0 order-1 md:order-2",
                    style: "min-height: 0; flex-grow: 1;",
                    GraphRepRender{
                        cyto_id: props.graph.cyto_id.clone(),
                        scope: props.graph.scope.clone(),
                        label: props.graph.label.clone(),
                        inner: props.graph.inner.clone(),
                        new_card_hook: props.graph.new_card_hook.clone(),
                        is_init: props.graph.is_init.clone(),
                    }
                }
            }
        }
    }
}
