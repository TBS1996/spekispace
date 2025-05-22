use std::{collections::HashMap, sync::Arc, time::Duration};

use dioxus::prelude::*;
use speki_core::{
    attribute::{AttrAction, AttrEvent, RefAttr},
    audio::AudioId,
    card::{CardId, TextData},
    ledger::{CardAction, CardEvent},
    AttributeId, Card, CardType,
};

use speki_web::{Node, NodeId, NodeMetadata};
use tracing::info;

use crate::{
    components::{
        backside::BackPutRender, cardref::CardRefRender, frontside::FrontPutRender,
        graph::GraphRepRender, BackPut, CardRef, CardTy, DropDownMenu, FrontPut, GraphRep,
        RenderDependents,
    },
    overlays::{
        card_selector::{CardSelector, MyClosure},
        yesno::Yesno,
        OverlayEnum,
    },
    APP, IS_SHORT,
};

#[component]
pub fn CardViewerRender(props: CardViewer) -> Element {
    info!("render cardviewer");

    let old_card: Option<CardId> = props.old_card.read().as_ref().map(|c| c.id());

    let history = {
        if let Some(card) = props.old_card.cloned() {
            card.history().clone()
        } else {
            speki_core::recall_rate::History::new(Default::default())
        }
    };

    let now = APP.read().inner().time_provider.current_time();

    rsx! {
        div {
            class: "flex-none p-2 box-border order-2",
            DisplayHistory { history, now }
            RenderInputs {
                editor:props.editor.clone(),
                dependents:props.dependents.clone(),
                graph:props.graph.clone(),
                save_hook:props.save_hook.clone(),
                is_done:props.is_done.clone(),
                old_card:props.old_card.clone(),
                old_meta:props.old_meta.clone(),
                tempnode:props.tempnode.clone(),
                overlay:props.overlay.clone(),
            }

            RenderDependencies { card_text: props.editor.front.text.clone(), card_id: old_card, dependencies: props.editor.dependencies.clone(), overlay: props.overlay.clone()}
            if let Some(card_id) = old_card {
                RenderDependents { card_id, overlay: props.overlay.clone(), hidden: false}
            }
        }
    }
}

/// Abstraction over a card that might exist or not yet.
/// like when you add a new dependency and before you save it you add a dependency to that again
/// then we need a way to represent on the graph the prev card even tho it's not saved
#[derive(Clone, PartialEq)]
pub enum TempNode {
    Old(CardId),
    New {
        id: NodeId,
        front: FrontPut,
        dependencies: Signal<Vec<Signal<Card>>>,
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
                let dependencies: Vec<_> = dependencies
                    .cloned()
                    .into_iter()
                    .map(|c| Node::Card(c.read().id()))
                    .collect();

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

/// All th einfo needed to create the actual card, similar to the cardeditor struct
/// except that this is guaranteed to have all the info needed to create a card
/// while the careditor can always be in an unfinished state
pub struct CardRep {
    ty: CardType,
    namespace: Option<CardId>,
    front_audio: Option<AudioId>,
    back_audio: Option<AudioId>,
    deps: Vec<CardId>,
    attrs: HashMap<AttributeId, String>,
}

/// container for all the structs you edit while creating/modifying a card
#[derive(Props, Clone)]
pub struct CardEditor {
    pub front: FrontPut,
    namespace: CardRef,
    back: BackPut,
    default_question: Signal<String>,
    concept: CardRef,
    dependencies: Signal<Vec<Signal<Card>>>,
    allowed_cards: Vec<CardTy>,
    attrs: Signal<Vec<(AttributeId, Signal<String>)>>,
}

impl CardEditor {
    fn into_cardrep(self) -> Option<CardRep> {
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

                CardType::Normal {
                    front: TextData::from_raw(&front),
                    back,
                }
            }
            CardTy::Class => {
                let parent_class = self.concept.selected_card().cloned();
                let back = backside.to_backside().filter(|x| !x.is_empty_text());

                CardType::Class {
                    name: TextData::from_raw(&front),
                    back,
                    parent_class,
                    default_question: {
                        let s = self.default_question.cloned();
                        if s.is_empty() {
                            None
                        } else {
                            Some(TextData::from_raw(&s))
                        }
                    },
                }
            }
            CardTy::Instance => {
                let class = self.concept.selected_card().cloned()?;
                let back = backside.to_backside();

                CardType::Instance {
                    name: TextData::from_raw(&front),
                    back,
                    class,
                }
            }
            CardTy::Unfinished => CardType::Unfinished {
                front: TextData::from_raw(&front),
            },
        };

        Some(CardRep {
            ty,
            attrs: self
                .attrs
                .cloned()
                .into_iter()
                .filter_map(|(id, pattern)| {
                    let pattern = pattern.cloned();
                    if pattern.contains("{}") {
                        Some((id, pattern))
                    } else {
                        None
                    }
                })
                .collect(),
            namespace: self.namespace.selected_card().cloned(),
            front_audio: self.front.audio.cloned().map(|audio| audio.id),
            back_audio: self.back.audio.cloned().map(|audio| audio.id),
            deps: self
                .dependencies
                .cloned()
                .into_iter()
                .map(|c| c.read().id())
                .collect(),
        })
    }
}

impl PartialEq for CardEditor {
    fn eq(&self, other: &Self) -> bool {
        self.front == other.front
            && self.back == other.back
            && self.concept == other.concept
            && self.dependencies == other.dependencies
            && self.allowed_cards == other.allowed_cards
    }
}

#[component]
fn RenderDependencies(
    card_text: Signal<String>,
    card_id: Option<CardId>,
    dependencies: Signal<Vec<Signal<Card>>>,
    overlay: Signal<Option<OverlayEnum>>,
) -> Element {
    let show_graph = "opacity-100 visible";

    let deps = dependencies.cloned();

    rsx! {
        div {
            class: "flex flex-col {show_graph} w-full h-auto bg-white p-2 shadow-md rounded-md overflow-y-auto",


            div {
                class: "flex items-center justify-between mb-2",

                h4 {
                    class: "font-bold",
                    "Dependencies"
                }

                    button {
                        class: "p-1 hover:bg-gray-200 hover:border-gray-400 border border-transparent rounded-md transition-colors",
                        onclick: move |_| {
                            let currcard = card_text.cloned();
                            let mut depsig = dependencies.clone();

                            let fun = MyClosure::new(move |card: Signal<Card>| {
                                async move {
                                    depsig.write().push(card);
                                }
                            });

                            spawn(async move {
                                let front = currcard.clone();
                                let mut props = CardSelector::dependency_picker(fun).with_default_search(front);
                                if let Some(id)  = card_id {
                                    props = props.with_forbidden_cards(vec![id]);
                                }
                                overlay.clone().set(Some(OverlayEnum::CardSelector(props)));
                            });
                        },
                        "➕"
                    }
                }

            for card in deps {
                button {
                    class: "mb-1 p-1 bg-gray-100 rounded-md text-left",
                    onclick: move|_|{
                        let card = card.clone();
                        spawn(async move{
                            let viewer = CardViewer::new_from_card(card, Default::default()).await;
                            overlay.clone().set(Some(OverlayEnum::CardViewer(viewer)));
                        });
                    },
                    "{card}"
                }
            }
        }
    }
}

/*

idea:

when selecting instannce

it should come up all the attributes from the parent classes (recursively), like it'll ask the asnwer to those questions
and if you answer it it'll create those attr cards

like if `person` has attribute when was {} born, where was {} born,
then when you add a new person instance it'll have those textfields for those questions so that you can create them easilyy that way


*/

#[derive(Props, Clone)]
pub struct CardViewer {
    pub editor: CardEditor,
    pub dependents: Signal<Vec<Node>>,
    pub graph: GraphRep,
    pub save_hook: Option<MyClosure>,
    pub is_done: Signal<bool>,
    pub old_card: Signal<Option<Card>>,
    pub old_meta: Signal<Option<NodeMetadata>>,
    pub tempnode: TempNode,
    pub overlay: Signal<Option<OverlayEnum>>,
}

impl PartialEq for CardViewer {
    fn eq(&self, other: &Self) -> bool {
        self.editor == other.editor
            && self.dependents == other.dependents
            && self.graph == other.graph
            && self.is_done == other.is_done
            && self.old_card == other.old_card
            && self.old_meta == other.old_meta
            && self.tempnode == other.tempnode
            && self.overlay == other.overlay
    }
}

impl CardViewer {
    pub fn with_hook(mut self, hook: MyClosure) -> Self {
        self.save_hook = Some(hook);
        self
    }

    pub fn with_front_text(self, text: String) -> Self {
        self.editor.front.text.clone().set(text);
        self
    }

    pub fn with_allowed_cards(mut self, allowed: Vec<CardTy>) -> Self {
        if allowed.is_empty() {
            return self;
        }
        self.editor.front.dropdown = DropDownMenu::new(allowed.clone(), None);
        self.editor.allowed_cards = allowed;
        self
    }

    pub fn with_dependents(mut self, deps: Vec<Node>) -> Self {
        self.dependents.extend(deps);
        self
    }

    fn select_closure(
        graph: GraphRep,
        front: FrontPut,
        dependencies: Signal<Vec<Signal<Card>>>,
        dependents: Signal<Vec<Node>>,
        meta: Option<NodeMetadata>,
    ) -> MyClosure {
        MyClosure::new(move |card: Signal<Card>| {
            let _graph = graph.clone();
            let _front = front.clone();
            let _meta = meta.clone();

            async move {
                let graph = _graph.clone();
                let front = _front.clone();
                let deps = dependencies.clone();
                let meta = _meta.clone();
                deps.clone()
                    .write()
                    .retain(|dep| dep.read().id() != card.read().id());
                //refresh_graph(graph, front, deps, dependents.clone(), meta);
            }
        })
    }

    fn deselect_closure(
        graph: GraphRep,
        front: FrontPut,
        dependencies: Signal<Vec<Signal<Card>>>,
        dependents: Signal<Vec<Node>>,
    ) -> MyClosure {
        MyClosure::new(move |card: Signal<Card>| {
            info!("ref card set ?");
            dependencies.clone().write().push(card);
            /*
            refresh_graph(
                graph.clone(),
                front.clone(),
                dependencies,
                dependents.clone(),
                None,
            );
            */
            async move {}
        })
    }

    pub async fn new_from_card(card: Signal<Card>, graph: GraphRep) -> Self {
        let tempnode = TempNode::Old(card.read().id());

        let raw_ty = card.read().clone_base();

        graph.new_set_card(card.cloned());

        let front = {
            let frnt = FrontPut::new(CardTy::from_ctype(card.read().card_type()));
            if let Some(id) = card.read().front_audio_id() {
                //let audio = APP.read().inner().provider.audios.load_item(id).await;
                //frnt.audio.clone().set(audio);
            }
            frnt.text.clone().set(raw_ty.data.raw_front());
            frnt
        };

        let back = {
            let bck =
                BackPut::new(raw_ty.data.backside().cloned()).with_dependents(tempnode.clone());
            if let Some(id) = card.read().back_audio_id() {
                //let audio = APP.read().inner().provider.audios.load_item(id).await;
                //bck.audio.clone().set(audio);
            }

            let back = match raw_ty.data.backside() {
                Some(b) if b.is_text() => b.to_string(),
                _ => String::new(),
            };

            bck.text.clone().set(back);
            bck
        };

        let graph = graph.with_label(front.text.clone());
        let dependents: Signal<Vec<Node>> = Signal::new_in_scope(Default::default(), ScopeId(3));
        let meta = NodeMetadata::from_card(card.clone(), true).await;

        let editor = {
            let concept = {
                let concept = CardRef::new()
                    .with_dependents(tempnode.clone())
                    .with_allowed(vec![CardTy::Class]);
                if let Some(class) = raw_ty.data.class() {
                    let class = APP.read().load_card(class).await;
                    concept.set_ref(class);
                }

                concept
            };

            let attrs: Vec<(AttributeId, Signal<String>)> = if card.read().is_class() {
                let ledger = APP.read().inner().provider.attrs.clone();

                let attrs: Vec<speki_core::Attribute> = ledger
                    .get_ref_cache(RefAttr::Class, card.read().id())
                    .into_iter()
                    .map(|attr_id| ledger.load(&attr_id).unwrap())
                    .collect();

                let mut map: Vec<(AttributeId, Signal<String>)> = Default::default();

                for attr in attrs {
                    map.push((attr.id, Signal::new_in_scope(attr.pattern, ScopeId::APP)));
                }
                map
            } else {
                Default::default()
            };

            let namespace = {
                let namespace = CardRef::new();

                if let Some(card) = card.read().namespace() {
                    let card = APP.read().load_card(card).await;
                    namespace.set_ref(card);
                }

                namespace
            };

            let dependencies: Signal<Vec<Signal<Card>>> = Signal::new_in_scope(
                card.read()
                    .dependencies()
                    .into_iter()
                    .map(|id| APP.read().load_card_sync(id))
                    .collect(),
                ScopeId(3),
            );

            let f = Self::deselect_closure(
                graph.clone(),
                front.clone(),
                dependencies.clone(),
                dependents,
            );

            let af = Self::select_closure(
                graph.clone(),
                front.clone(),
                dependencies,
                dependents,
                Some(meta.clone()),
            );

            let bck = back.on_select(f.clone()).on_deselect(af.clone());
            let concept = concept.on_select(f.clone()).on_deselect(af.clone());

            let default_question = if let CardType::Class {
                default_question, ..
            } = card.read().clone_base().data
            {
                default_question.unwrap_or_default().to_raw()
            } else {
                String::new()
            };

            CardEditor {
                front,
                attrs: Signal::new_in_scope(attrs, ScopeId::APP),
                namespace,
                back: bck,
                concept,
                dependencies,
                allowed_cards: vec![],
                default_question: Signal::new_in_scope(default_question, ScopeId::APP),
            }
        };

        Self {
            editor,
            dependents,
            graph,
            is_done: Signal::new_in_scope(false, ScopeId(3)),
            old_card: Signal::new_in_scope(Some(card.cloned()), ScopeId(3)),
            save_hook: None,
            tempnode,
            old_meta: Signal::new_in_scope(Some(meta), ScopeId::APP),
            overlay: Signal::new_in_scope(Default::default(), ScopeId::APP),
        }
    }

    pub fn new() -> Self {
        let front = FrontPut::new(CardTy::Normal);
        let dependencies: Signal<Vec<Signal<Card>>> =
            Signal::new_in_scope(Default::default(), ScopeId::APP);
        let dependents = Signal::new_in_scope(Default::default(), ScopeId(3));
        let graph = GraphRep::default().with_label(front.text.clone());

        let tempnode = TempNode::New {
            id: NodeId::new_temp(),
            front: front.clone(),
            dependencies: dependencies.clone(),
            dependents: dependents.clone(),
        };

        let editor = {
            let af =
                Self::select_closure(graph.clone(), front.clone(), dependencies, dependents, None);
            let f = Self::deselect_closure(graph.clone(), front.clone(), dependencies, dependents);

            let back = BackPut::new(None)
                .with_dependents(tempnode.clone())
                .on_select(f.clone())
                .on_deselect(af.clone());

            let concept = CardRef::new()
                .with_dependents(tempnode.clone())
                .with_allowed(vec![CardTy::Class])
                .on_select(f.clone())
                .on_deselect(af.clone());

            CardEditor {
                front,
                namespace: CardRef::new(),
                back,
                concept,
                dependencies,
                allowed_cards: vec![],
                default_question: Signal::new_in_scope(String::new(), ScopeId::APP),
                attrs: Default::default(),
            }
        };

        let selv = Self {
            editor,
            graph,
            is_done: Signal::new_in_scope(false, ScopeId(3)),
            old_card: Signal::new_in_scope(None, ScopeId(3)),
            save_hook: None,
            dependents,
            tempnode,
            old_meta: Signal::new_in_scope(None, ScopeId::APP),
            overlay: Signal::new_in_scope(Default::default(), ScopeId::APP),
        };

        selv.set_graph();
        selv
    }

    pub fn with_dependency(mut self, dep: CardId) -> Self {
        let card = APP.read().load_card_sync(dep);
        self.editor.dependencies.push(card);
        self
    }

    async fn reset(&self) {
        self.editor.front.reset();
        self.editor.back.reset();
        self.editor.concept.reset();
        self.editor.dependencies.clone().write().clear();
        self.old_card.clone().set(None);
        self.graph.clear().await;
    }

    pub fn set_graph(&self) {
        if let Some(card) = self.old_card.cloned() {
            self.graph.new_set_card(card.clone());
            return;
        }

        /*
        refresh_graph(
            self.graph.clone(),
            self.editor.front.clone(),
            self.editor.dependencies.clone(),
            self.dependents.clone(),
            self.old_meta.cloned(),
        );
        */
    }
}

#[component]
fn RenderInputs(props: CardViewer) -> Element {
    info!("render inputs");
    let ty = props.editor.front.dropdown.selected.clone();
    let card_id = props.old_card.read().as_ref().map(|c| c.id());

    rsx! {
        div {
            InputElements {
                front: props.editor.front.clone(),
                back: props.editor.back.clone(),
                default_question: props.editor.default_question.clone(),
                concept: props.editor.concept.clone(),
                overlay: props.overlay.clone(),
                ty: ty.cloned(),
                card_id,
                namespace: props.editor.namespace.clone(),
                attrs: props.editor.attrs.clone(),
            }
        }
        div {
            if let Some(card) = props.old_card.cloned() {
                DeleteButton{card: card.id(), isdone: props.is_done.clone(), overlay: props.overlay.clone()}
                Suspend { card: props.old_card.clone() }
            }

            add_dep { selv: props.clone() }

            save_button { CardViewer: props.clone() }
        }
    }
}

#[component]
fn InputElements(
    front: FrontPut,
    back: BackPut,
    default_question: Signal<String>,
    concept: CardRef,
    overlay: Signal<Option<OverlayEnum>>,
    ty: CardTy,
    card_id: Option<CardId>,
    namespace: CardRef,
    attrs: Signal<Vec<(AttributeId, Signal<String>)>>,
) -> Element {
    let is_short = IS_SHORT.cloned();

    let has_attrs = !attrs.is_empty();

    dbg!(&attrs);

    let is_class = matches!(ty, CardTy::Class);
    let inner_attrs = attrs.cloned();

    rsx! {
        FrontPutRender { dropdown: front.dropdown.clone(), text: front.text.clone(), audio: front.audio.clone(), overlay: overlay.clone() }

        div {
            class: "block text-gray-700 text-sm font-medium mb-2",
            style: "margin-right: 82px;",

            CardRefRender{
                card_display: namespace.display.clone(),
                selected_card: namespace.card.clone(),
                placeholder: "choose namespace",
                on_select: namespace.on_select.clone(),
                on_deselect: namespace.on_deselect.clone(),
                dependent: namespace.dependent.clone(),
                allowed: namespace.allowed.clone(),
                overlay: overlay.clone(),
            },
        }

        match ty {
            CardTy::Unfinished => rsx! {},

            CardTy::Normal => rsx! {
                BackPutRender {
                    text: back.text.clone(),
                    dropdown: back.dropdown.clone(),
                    ref_card: back.ref_card.clone(),
                    overlay: overlay.clone(),
                    audio: back.audio.clone(),
                }
            },
            CardTy::Class => rsx! {

                BackPutRender {
                    text: back.text.clone(),
                    dropdown: back.dropdown.clone(),
                    ref_card: back.ref_card.clone(),
                    overlay: overlay.clone(),
                    audio: back.audio.clone(),
                }

                input {
                    class: "bg-white w-full border border-gray-300 rounded-md p-2 mb-4 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                    value: "{default_question}",
                    placeholder: "default question",
                    oninput: move |evt| default_question.set(evt.value()),
                }

                div {
                    class: "block text-gray-700 text-sm font-medium mb-2",
                    style: "margin-right: 82px;",
                    "Parent class"

                    CardRefRender{
                        card_display: concept.display.clone(),
                        selected_card: concept.card.clone(),
                        placeholder: "pick parent class",
                        on_select: concept.on_select.clone(),
                        on_deselect: concept.on_deselect.clone(),
                        dependent: concept.dependent.clone(),
                        allowed: concept.allowed.clone(),
                        overlay: overlay.clone(),
                    },
                }
            },
            CardTy::Instance => rsx! {
                BackPutRender {
                    text: back.text.clone(),
                    dropdown: back.dropdown.clone(),
                    ref_card: back.ref_card.clone(),
                    overlay: overlay.clone(),
                    audio: back.audio.clone(),
                }

                if !is_short {
                    div {
                        class: "block text-gray-700 text-sm font-medium mb-2",
                        style: "margin-right: 81px;",
                        "Class of instance"
                        CardRefRender{
                            card_display: concept.display.clone(),
                            selected_card: concept.card.clone(),
                            placeholder: "pick class of instance",
                            on_select: concept.on_select.clone(),
                            on_deselect: concept.on_deselect.clone(),
                            dependent: concept.dependent.clone(),
                            allowed: concept.allowed.clone(),
                            overlay: overlay.clone(),
                        },
                    }
                } else {
                    div {
                        class: "block text-gray-700 text-sm font-medium",
                        style: "margin-right: 81px;",
                        CardRefRender{
                            card_display: concept.display.clone(),
                            selected_card: concept.card.clone(),
                            placeholder: "pick class of instance",
                            on_select: concept.on_select.clone(),
                            on_deselect: concept.on_deselect.clone(),
                            dependent: concept.dependent.clone(),
                            allowed: concept.allowed.clone(),
                            overlay: overlay.clone(),
                        },
                    }
                }
            },
        }

        if has_attrs {
            p {"attributes"}
            div {
                class: "max-h-64 overflow-y-auto",
                for (_id, mut pattern) in inner_attrs {
                    input {
                        class: "bg-white w-full border border-gray-300 rounded-md p-2 mb-4 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                        value: "{pattern}",
                        placeholder: "default question",
                        oninput: move |evt| pattern.set(evt.value()),
                    }
                }
            }
        }


        if is_class {
            button {
                class: "mt-2 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base md:mt-0",
                onclick: move |_| {
                    attrs.write().push((AttributeId::new_v4(), Signal::new_in_scope("{}".to_string(), ScopeId::APP)));
                },
                "add attribute"

             }
        }
    }
}

use speki_core::recall_rate::History as MyHistory;

fn hr_dur(dur: Duration) -> String {
    let secs = dur.as_secs();

    if secs > 86400 {
        format!("{:.2}d", secs as f32 / 86400.)
    } else if secs > 3600 {
        format!("{:.2}h", secs as f32 / 3600.)
    } else if secs > 60 {
        format!("{:.2}m", secs as f32 / 60.)
    } else {
        format!("{}", secs)
    }
}

#[component]
fn DisplayHistory(history: MyHistory, now: Duration) -> Element {
    let output = if history.reviews.is_empty() {
        format!("no review history!!")
    } else {
        let mut output = format!("review history: ");
        for review in history.reviews {
            let dur = now - review.timestamp;
            output.push_str(&format!(" {},", hr_dur(dur)));
        }

        output
    };

    rsx! {
        p{"{output}"}
    }
}

#[component]
fn DeleteButton(
    card: CardId,
    isdone: Signal<bool>,
    overlay: Signal<Option<OverlayEnum>>,
) -> Element {
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

#[component]
fn Suspend(card: Signal<Option<Card>>) -> Element {
    let Some(card) = card.cloned() else {
        return rsx! {};
    };

    let is_suspended = card.is_suspended();
    let txt = if is_suspended { "unsuspend" } else { "suspend" };

    rsx! {
        button {
            class: "mt-2 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base md:mt-0",
            onclick: move |_| {
                let mut card = card.clone();
                spawn(async move {
                    card.set_suspend(!is_suspended).await;
                });
            },
            "{txt}"
        }
    }
}

#[component]
fn save_button(CardViewer: CardViewer) -> Element {
    let selv = CardViewer.clone();

    let is_new = CardViewer.old_card.as_ref().is_none();

    let enabled = selv.editor.clone().into_cardrep().is_some_and(|card| {
        CardViewer
            .editor
            .allowed_cards
            .contains(&CardTy::from_ctype(card.ty.fieldless()))
            || CardViewer.editor.allowed_cards.is_empty()
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
                if let Some(card) = selv.editor.clone().into_cardrep() {
                    let selveste = selv.clone();
                    spawn(async move {


                        let mut events: Vec<CardEvent> = vec![];

                        let id = selveste.old_card.cloned().map(|card|card.id()).unwrap_or_else(CardId::new_v4);
                        events.push(CardEvent::new(id, CardAction::UpsertCard(card.ty)));
                        events.push(CardEvent::new(id, CardAction::SetFrontAudio (card.front_audio)));
                        events.push(CardEvent::new(id, CardAction::SetBackAudio ( card.back_audio)));
                        events.push(CardEvent::new(id, CardAction::SetNamespace ( card.namespace)));

                        for dep in card.deps {
                            events.push(CardEvent::new(id, CardAction::AddDependency(dep)));
                        }

                        if let Some(audio) = selv.editor.front.audio.cloned() {
                            //APP.read().inner().provider.audios.save_item(audio).await;
                        }
                        if let Some(audio) = selv.editor.back.audio.cloned() {
                            //APP.read().inner().provider.audios.save_item(audio).await;
                        }

                        for event in events {
                            APP.read().inner().provider.cards.insert_ledger(event);
                        }

                        for (attr, pattern) in card.attrs.into_iter() {
                            APP.read().inner().provider.attrs.insert_ledger(AttrEvent{
                                id: attr,
                                action: AttrAction::UpSert { pattern, class: id},
                            });
                        }

                        let card = APP.read().inner().card_provider().load(id).unwrap();
                        let inner_card = Arc::unwrap_or_clone(card);
                        let card = Signal::new_in_scope(inner_card.clone(),  ScopeId::APP);
                        if let Some(hook) = selveste.save_hook.clone() {
                            hook.call(card).await;
                        }
                        if let Some(card) = selveste.old_card.cloned() {
                            info!("setting updated card: {:?}", inner_card);
                            //card.set(inner_card);
                        }

                        selveste.reset().await;
                        selv.is_done.clone().set(true);
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

#[component]
fn add_dep(selv: CardViewer) -> Element {
    let selv = selv.clone();
    let front = selv.editor.front.text.cloned();
    let overlay = selv.overlay.clone();
    rsx! {
        button {
            class: "mt-2 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base md:mt-0",
            onclick: move |_| {
                let selv = selv.clone();
                let selv2 = selv.clone();

                let fun = MyClosure::new(
                    move |card: Signal<Card>| {
                    selv.editor.dependencies.clone().write().push(card);
                    selv.set_graph();
                    let old_card = selv.old_card.cloned();
                    async move {
                        if let Some(mut old_card) = old_card {
                            old_card.add_dependency(card.read().id()).await;
                        }
                    }
                }


            );

                info!("1 scope is ! {:?}", current_scope_id().unwrap());
                let thefront = front.clone();

                spawn(async move {
                    let dependent: Node = selv2.tempnode.clone().into();
                    let props = CardSelector::dependency_picker(fun).with_dependents(vec![dependent]).with_default_search(thefront.clone());
                    overlay.clone().set(Some(OverlayEnum::CardSelector(props)));
                    info!("2 scope is ! {:?}", current_scope_id().unwrap());
                });
            },
            "add dependency"
        }
    }
}
