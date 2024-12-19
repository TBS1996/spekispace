use std::sync::Arc;

use dioxus::prelude::*;
use speki_core::{AnyType, Card, ClassCard, InstanceCard, NormalCard, UnfinishedCard};
use speki_dto::CardId;
use speki_web::NodeMetadata;
use tracing::info;
use uuid::Uuid;

use crate::{
    components::{BackPut, CardRef, CardTy, FrontPut, GraphRep},
    overlays::card_selector::CardSelector,
    overlays::{PopTray, Popup},
    utils::{App, CardEntries},
    OVERLAY,
};

use crate::components::Komponent;

pub struct CardRep {
    ty: AnyType,
    deps: Vec<CardId>,
}

#[derive(Clone)]
pub struct CardViewer {
    app: App,
    front: FrontPut,
    back: BackPut,
    concept: CardRef,
    dependencies: Signal<Vec<CardId>>,
    graph: GraphRep,
    save_hook: Option<Arc<Box<dyn Fn(Arc<Card<AnyType>>)>>>,
    is_done: Signal<bool>,
    old_card: Signal<Option<Arc<Card<AnyType>>>>,
}

impl CardViewer {
    pub async fn _new_from_card(
        card: Arc<Card<AnyType>>,
        graph: GraphRep,
        app: App,
        entries: CardEntries,
    ) -> Self {
        let dependencies = card.dependency_ids().await;
        let raw = card.to_raw();
        let front = raw.data.front.unwrap_or_default();
        let back = raw.data.back.unwrap_or_default().to_string();
        let frnt = FrontPut::new();
        frnt.text.clone().set(front);
        let bck = BackPut::new();
        bck.text.clone().set(back);

        Self {
            app,
            front: frnt,
            back: bck,
            dependencies: Signal::new_in_scope(dependencies.into_iter().collect(), ScopeId(3)),
            graph,
            is_done: Signal::new_in_scope(false, ScopeId(3)),
            concept: CardRef::new(entries.classes.clone()),
            old_card: Signal::new_in_scope(Some(card), ScopeId(3)),
            save_hook: None,
        }
    }

    pub fn new(graph: GraphRep, app: App, entries: CardEntries) -> Self {
        Self {
            app,
            front: FrontPut::new(),
            back: BackPut::new(),
            dependencies: Default::default(),
            graph,
            is_done: Signal::new_in_scope(false, ScopeId(3)),
            concept: CardRef::new(entries.classes.clone()),
            old_card: Signal::new_in_scope(None, ScopeId(3)),
            save_hook: None,
        }
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
        self.graph.new_set_card_rep(node, dependencies);
    }

    fn to_node(&self) -> NodeMetadata {
        NodeMetadata {
            id: Uuid::default().to_string(),
            label: self.front.text.cloned(),
            color: "#42cbf5".to_string(),
            ty: speki_dto::CType::Normal,
        }
    }
}

impl Komponent for CardViewer {
    fn render(&self) -> Element {
        info!(" XX rendering display_card");

        let selv = self.clone();
        let selv2 = self.clone();
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
                            let selv = selv2.clone();

                            let fun = move |card: Arc<Card<AnyType>>| {
                                selv.dependencies.clone().write().push(card.id);
                                selv.set_graph();
                            };

                            let props = CardSelector::dependency_picker(Box::new(fun));

                            let pop: Popup = Box::new(props);
                            OVERLAY.cloned().set(pop);
                    },
                    "add dependency"
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

                                let card = Arc::new(selveste.app.0.new_from_raw(new_raw).await);
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
            { self.graph.render() }
        }
    }
}

impl PopTray for CardViewer {
    fn is_done(&self) -> Signal<bool> {
        self.is_done.clone()
    }
}
