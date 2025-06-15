use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
    time::Duration,
};

use dioxus::prelude::*;
use either::Either;
use ledgerstore::TheLedgerEvent;
use speki_core::{
    audio::AudioId,
    card::{AttributeId, Attrv2, BackSide, CardId, TextData},
    collection::DynCard,
    ledger::{CardAction, CardEvent},
    set::SetExpr,
    Card, CardType, RefType,
};

use speki_web::{Node, NodeId, NodeMetadata};
use tracing::info;

use crate::{
    append_overlay, ask_openai,
    components::{
        backside::BackPutRender, cardref::CardRefRender, frontside::FrontPutRender, BackPut,
        CardRef, CardTy, DropDownMenu, FrontPut, RenderDependents,
    },
    load_api_key,
    overlays::{
        card_selector::{CardSelector, MyClosure},
        OverlayEnum,
    },
    pop_overlay, APP,
};

#[component]
pub fn CardViewerRender(props: CardViewer) -> Element {
    info!("render cardviewer");

    let old_card: Option<CardId> = props.old_card.read().as_ref().map(|c| c.id());
    let history = match props.old_card.cloned() {
        Some(card) => Some(card.history().to_owned()),
        None => None,
    };

    let now = APP.read().inner().time_provider.current_time();

    rsx! {
        div {
            class: "flex-none p-2 box-border order-2",
            if let Some(history) = history {
                DisplayHistory { history, now }
            }

            RenderInputs {
                editor:props.editor.clone(),
                dependents:props.dependents.clone(),
                save_hook:props.save_hook.clone(),
                old_card:props.old_card.clone(),
                old_meta:props.old_meta.clone(),
                tempnode:props.tempnode.clone(),
            }

            RenderDependencies { card_text: props.editor.front.text.clone(), card_id: old_card, dependencies: props.editor.dependencies.clone()}
            if let Some(card_id) = old_card {
                RenderDependents { card_id, hidden: false}
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

/// All th einfo needed to create the actual card, similar to the cardeditor struct
/// except that this is guaranteed to have all the info needed to create a card
/// while the careditor can always be in an unfinished state
pub struct CardRep {
    ty: CardType,
    namespace: Option<CardId>,
    front_audio: Option<AudioId>,
    back_audio: Option<AudioId>,
    deps: Vec<CardId>,
    answered_attrs: Vec<AttrAnswer>,
}

#[derive(Clone, Debug)]
enum AttrAnswer {
    /// There's already a card created for this attribute
    /// answer can be modified but not changed
    Old {
        id: CardId,
        attr_id: AttributeId,
        question: String,
        answer: Either<BackPut, CardRef>,
    },
    /// There's not already one, so you can create it.
    /// the id now referes the attribute not the card.
    New {
        attr_id: Attrv2,
        question: String,
        answer: Signal<Option<Either<BackPut, CardRef>>>,
    },
}

impl PartialEq for AttrAnswer {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Old {
                    id: l_id,
                    question: l_question,
                    answer: l_answer,
                    attr_id: l_attr_id,
                },
                Self::Old {
                    id: r_id,
                    question: r_question,
                    answer: r_answer,
                    attr_id: r_attr_id,
                },
            ) => {
                l_id == r_id
                    && l_question == r_question
                    && l_answer == r_answer
                    && l_attr_id == r_attr_id
            }
            (
                Self::New {
                    attr_id: l_attr_id,
                    question: l_question,
                    answer: l_answer,
                },
                Self::New {
                    attr_id: r_attr_id,
                    question: r_question,
                    answer: r_answer,
                },
            ) => l_attr_id == r_attr_id && l_question == r_question && l_answer == r_answer,
            _ => false,
        }
    }
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
    attrs: Signal<Vec<(AttributeId, (Signal<String>, CardRef))>>,
    attr_answers: Signal<Vec<AttrAnswer>>,
}

impl CardEditor {
    fn into_cardrep(self) -> Option<CardRep> {
        let backside = self.back.clone();
        let frontside = self.front.clone();

        let front = format!("{}", frontside.text.cloned());

        if front.is_empty() {
            return None;
        }

        let attrs: HashMap<AttributeId, (String, Option<CardId>)> = self
            .attrs
            .cloned()
            .into_iter()
            .filter_map(|(id, (pattern, answerty))| {
                let pattern = pattern.cloned();
                let answerty = answerty.selected_card().cloned();
                if pattern.contains("{}") {
                    Some((id, (pattern, answerty)))
                } else {
                    None
                }
            })
            .collect();

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
                let attrs: Vec<Attrv2> = attrs
                    .into_iter()
                    .map(|(id, (pattern, back_type))| Attrv2 {
                        id,
                        pattern,
                        back_type,
                    })
                    .collect();

                CardType::Class {
                    name: TextData::from_raw(&front),
                    back,
                    parent_class,
                    attrs,
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
            answered_attrs: self.attr_answers.cloned(),
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
                    "Explicit dependencies"
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
                                append_overlay(OverlayEnum::CardSelector(props));
                            });
                        },
                        "➕"
                    }
                }

            for (idx, card) in deps.into_iter().enumerate() {
                div {
                    class: "flex flex-row",
                button {
                    class: "mb-1 p-1 bg-gray-100 rounded-md text-left",
                    onclick: move|_|{
                        let card = card.clone();
                        spawn(async move{
                            let viewer = CardViewer::new_from_card(card).await;
                            append_overlay(OverlayEnum::CardViewer(viewer));
                        });
                    },
                    "{card}"
                }

                button {
                    class: "p-1 hover:bg-gray-200 hover:border-gray-400 border border-transparent rounded-md transition-colors",
                    onclick: move |_|{
                        let removed =  dependencies.write().remove(idx);
                        if let Some(id) = card_id {
                            let event = TheLedgerEvent::new(id, CardAction::RemoveDependency(removed.read().id()));
                            APP.read().inner().provider.cards.insert_ledger(event).unwrap();
                        }
                    },
                    "X"
                }


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
    pub save_hook: Option<MyClosure>,
    pub old_card: Signal<Option<Card>>,
    pub old_meta: Signal<Option<NodeMetadata>>,
    pub tempnode: TempNode,
}

impl PartialEq for CardViewer {
    fn eq(&self, other: &Self) -> bool {
        self.editor == other.editor
            && self.dependents == other.dependents
            && self.old_card == other.old_card
            && self.old_meta == other.old_meta
            && self.tempnode == other.tempnode
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
        front: FrontPut,
        dependencies: Signal<Vec<Signal<Card>>>,
        meta: Option<NodeMetadata>,
    ) -> MyClosure {
        MyClosure::new(move |card: Signal<Card>| {
            let _front = front.clone();
            let _meta = meta.clone();

            async move {
                let deps = dependencies.clone();
                deps.clone()
                    .write()
                    .retain(|dep| dep.read().id() != card.read().id());
            }
        })
    }

    fn deselect_closure(dependencies: Signal<Vec<Signal<Card>>>) -> MyClosure {
        MyClosure::new(move |card: Signal<Card>| {
            info!("ref card set ?");
            dependencies.clone().write().push(card);
            async move {}
        })
    }

    pub async fn new_from_card(mut card: Signal<Card>) -> Self {
        if card.read().is_attribute() {
            let instance = card.read().attribute_instance();
            card = APP.read().load_card_sync(instance);
        }

        let tempnode = TempNode::Old(card.read().id());

        let raw_ty = card.read().clone_base();

        let front = {
            let frnt = FrontPut::new(CardTy::from_ctype(card.read().card_type()));
            frnt.text.clone().set(raw_ty.data.raw_front());
            frnt
        };

        let back = {
            let bck =
                BackPut::new(raw_ty.data.backside().cloned()).with_dependents(tempnode.clone());

            let back = match raw_ty.data.backside() {
                Some(BackSide::Time(ts)) => ts.to_string(),
                Some(b) if b.is_text() => b.to_string(),
                _ => String::new(),
            };

            bck.text.clone().set(back);
            bck
        };

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

            // for instance cards, how you answer certain attributes.

            let attrs = if card.read().is_instance() {
                let curr_card = card.clone();

                let mut attrs: Vec<Attrv2> = curr_card.read().attributes().unwrap_or_default();

                let provider = APP.read().inner().card_provider.clone();
                let card_ledger = APP.read().inner().provider.cards.clone();

                // all cards that are an attribute card based on a given instance.
                // wait, isnt this all we need? damn..
                let attr_cards_based_on_instance: BTreeSet<Arc<Card>> = card_ledger
                    .get_ref_cache(RefType::AttrClass, curr_card.read().id())
                    .into_iter()
                    .map(|id| provider.load(id.parse().unwrap()).unwrap())
                    .collect();

                attrs.retain(|attr_id| {
                    !attr_cards_based_on_instance
                        .iter()
                        .any(|card| card.uses_attr_id(attr_id.id))
                });

                let mut output: Vec<AttrAnswer> = vec![];

                for card in attr_cards_based_on_instance {
                    let attr_id = card.attr_id().unwrap();
                    let instance = card.attribute_instance();
                    let instance = APP.read().inner().card_provider.load(instance).unwrap();
                    let attr = instance.get_attr(attr_id).unwrap();

                    let answer = match attr.back_type {
                        Some(card_id) => {
                            let filter = DynCard::Instances(card_id);
                            let mut cref = CardRef::new();
                            cref.filter = SetExpr::union_with([filter]);
                            if let Some(selected_card) =
                                card.back_side().and_then(|bs| bs.as_card())
                            {
                                cref.set_ref_id(selected_card);
                            };
                            Either::Right(cref)
                        }
                        None => {
                            let b = BackPut::new(card.clone_base().data.backside().cloned());
                            Either::Left(b)
                        }
                    };

                    let question = card.front_side().to_string();
                    let val = AttrAnswer::Old {
                        attr_id: card.attr_id().unwrap(),
                        id: card.id(),
                        question,
                        answer,
                    };
                    output.push(val);
                }

                for attr in attrs {
                    let instance = card.read().name_textdata().to_raw();

                    let question = attr.pattern.replace("{}", &instance);

                    let val = AttrAnswer::New {
                        attr_id: attr,
                        question,
                        answer: Signal::new_in_scope(None, ScopeId::APP),
                    };
                    output.push(val);
                }

                /*

                1. create a set of all the attributes valid for this instance
                2. create a set of all the attribute cards that reference an attribute in previous set
                3. remove all the attributes from the first set that have a matching card in second set
                -> now we'll two sets that together form all the valid attributes, but ones meaning is all the ones created, the other is the ones not (yet) created.
                4. list the inputs of the created ones so the user can easily change the provided answer.
                5. for the ones not created, no input box but a button where if you press it one will be created. user can write answer there. should be possible to X it out.

                i think to find the right attr card, i need to get all the cards whose attribute belong to a class, and all the attrcards belonging to an instance, and then find the one
                card that is in both sets?

                 */

                output
            } else {
                vec![]
            };

            let attr_answers = Signal::new_in_scope(attrs, ScopeId::APP);

            // The attributes for a given class
            let attrs: Vec<(AttributeId, (Signal<String>, CardRef))> = if card.read().is_class() {
                let attrs = card.read().attributes().unwrap();

                let mut map: Vec<(AttributeId, (Signal<String>, CardRef))> = Default::default();

                for attr in attrs {
                    let cref = CardRef::new();

                    if let Some(ty) = attr.back_type {
                        let card = APP.read().load_card_sync(ty);
                        cref.set_ref(card);
                    }

                    map.push((
                        attr.id,
                        (Signal::new_in_scope(attr.pattern, ScopeId::APP), cref),
                    ));
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
                    .explicit_dependencies()
                    .into_iter()
                    .map(|id| APP.read().load_card_sync(id))
                    .collect(),
                ScopeId(3),
            );

            let f = Self::deselect_closure(dependencies.clone());

            let af = Self::select_closure(front.clone(), dependencies, Some(meta.clone()));

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
                attr_answers,
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
            old_card: Signal::new_in_scope(Some(card.cloned()), ScopeId(3)),
            save_hook: None,
            tempnode,
            old_meta: Signal::new_in_scope(Some(meta), ScopeId::APP),
        }
    }

    pub fn new() -> Self {
        let front = FrontPut::new(CardTy::Normal);
        let dependencies: Signal<Vec<Signal<Card>>> =
            Signal::new_in_scope(Default::default(), ScopeId::APP);
        let dependents = Signal::new_in_scope(Default::default(), ScopeId(3));

        let tempnode = TempNode::New {
            id: NodeId::new_temp(),
            front: front.clone(),
            dependencies: dependencies.clone(),
            dependents: dependents.clone(),
        };

        let editor = {
            let af = Self::select_closure(front.clone(), dependencies, None);
            let f = Self::deselect_closure(dependencies);

            let back = BackPut::new(None)
                .with_dependents(tempnode.clone())
                .on_select(f.clone())
                .on_deselect(af.clone());

            let concept = CardRef::new()
                .with_dependents(tempnode.clone())
                .with_allowed(vec![CardTy::Class])
                .on_select(f.clone())
                .on_deselect(af.clone());

            let attr_answers = Signal::new_in_scope(Default::default(), ScopeId::APP);

            CardEditor {
                attr_answers,
                front,
                namespace: CardRef::new(),
                back,
                concept,
                dependencies,
                allowed_cards: vec![],
                default_question: Signal::new_in_scope(String::new(), ScopeId::APP),
                attrs: Signal::new_in_scope(Default::default(), ScopeId::APP),
            }
        };

        Self {
            editor,
            old_card: Signal::new_in_scope(None, ScopeId(3)),
            save_hook: None,
            dependents,
            tempnode,
            old_meta: Signal::new_in_scope(None, ScopeId::APP),
        }
    }

    pub fn with_dependency(mut self, dep: CardId) -> Self {
        let card = APP.read().load_card_sync(dep);
        self.editor.dependencies.push(card);
        self
    }

    async fn reset(&self) {
        self.editor.front.reset();
        self.editor.back.reset();
        self.editor.dependencies.clone().write().clear();
        self.old_card.clone().set(None);
    }
}

#[component]
fn RenderInputs(props: CardViewer) -> Element {
    info!("render inputs");
    let ty = props.editor.front.dropdown.selected.clone();
    let card_id = props.old_card.read().as_ref().map(|c| c.id());
    let deletable = match props.old_card.cloned() {
        Some(card) => card.dependents().is_empty(),
        None => false,
    };

    let api_key = load_api_key();
    let ai_enabled = api_key.is_some() && !props.editor.front.text.read().trim().is_empty();

    rsx! {
        div {
            InputElements {
                front: props.editor.front.clone(),
                back: props.editor.back.clone(),
                default_question: props.editor.default_question.clone(),
                concept: props.editor.concept.clone(),
                ty: ty.cloned(),
                card_id,
                namespace: props.editor.namespace.clone(),
                attrs: props.editor.attrs.clone(),
                attr_answers: props.editor.attr_answers.clone(),
            }
        }
        div {
            if let Some(card) = props.old_card.cloned() {
                if deletable {
                    DeleteButton{card: card.id()}
                }
                Suspend { card: props.old_card.clone() }
            }

            add_dep { selv: props.clone() }

            save_button { CardViewer: props.clone() }

            if ai_enabled {
                AiComplete { CardViewer: props.clone(), api_key }
            }
        }
    }
}

#[component]
fn InputElements(
    front: FrontPut,
    back: BackPut,
    default_question: Signal<String>,
    concept: CardRef,
    ty: CardTy,
    card_id: Option<CardId>,
    namespace: CardRef,
    attrs: Signal<Vec<(AttributeId, (Signal<String>, CardRef))>>,
    attr_answers: Signal<Vec<AttrAnswer>>,
) -> Element {
    let has_attrs = !attrs.is_empty();
    let has_attr_answers = !attr_answers.read().is_empty();

    let is_class = matches!(ty, CardTy::Class);
    let inner_attrs = attrs.cloned();

    rsx! {
        FrontPutRender { dropdown: front.dropdown.clone(), text: front.text.clone(), audio: front.audio.clone() }

        div {
            class: "block text-gray-700 text-sm font-medium mb-2",
            style: "margin-right: 82px;",

            CardRefRender{
                selected_card: namespace.card.clone(),
                placeholder: "choose namespace",
                on_select: namespace.on_select.clone(),
                on_deselect: namespace.on_deselect.clone(),
                dependent: namespace.dependent.clone(),
                allowed: namespace.allowed.clone(),
                filter: namespace.filter.clone(),
            },
        }

        match ty {
            CardTy::Unfinished => rsx! {},

            CardTy::Normal => rsx! {
                BackPutRender {
                    text: back.text.clone(),
                    dropdown: back.dropdown.clone(),
                    ref_card: back.ref_card.clone(),
                    audio: back.audio.clone(),
                }
            },
            CardTy::Class => rsx! {

                BackPutRender {
                    text: back.text.clone(),
                    dropdown: back.dropdown.clone(),
                    ref_card: back.ref_card.clone(),
                    audio: back.audio.clone(),
                }

                div {
                    class: "block text-gray-700 text-sm font-medium mb-2",
                    style: "margin-right: 82px;",
                    "Parent class"

                    CardRefRender{
                        selected_card: concept.card.clone(),
                        placeholder: "pick parent class",
                        on_select: concept.on_select.clone(),
                        on_deselect: concept.on_deselect.clone(),
                        dependent: concept.dependent.clone(),
                        allowed: concept.allowed.clone(),
                        filter: concept.filter.clone(),
                    },


                    }
            },
            CardTy::Instance => rsx! {
                BackPutRender {
                    text: back.text.clone(),
                    dropdown: back.dropdown.clone(),
                    ref_card: back.ref_card.clone(),
                    audio: back.audio.clone(),
                }

                div {
                    class: "block text-gray-700 text-sm font-medium mb-2",
                    style: "margin-right: 81px;",
                    "Class of instance"
                    CardRefRender{
                        selected_card: concept.card.clone(),
                        placeholder: "pick class of instance",
                        on_select: concept.on_select.clone(),
                        on_deselect: concept.on_deselect.clone(),
                        dependent: concept.dependent.clone(),
                        allowed: concept.allowed.clone(),
                        filter: concept.filter.clone(),
                    },
                }

                if has_attr_answers {

            p {"attributes"}
            div {
                class: "max-h-64 overflow-y-auto",

                    for answer in attr_answers.iter() {
                        match answer.clone() {
                            AttrAnswer::Old {question, answer,..} => {
                                match answer {
                                    Either::Left(answer) => {
                                        rsx! {
                                            p {"{question}"}
                                            BackPutRender {
                                                text: answer.text.clone(),
                                                dropdown: answer.dropdown.clone(),
                                                ref_card: answer.ref_card.clone(),
                                                audio: answer.audio.clone(),
                                            }
                                        }

                                    },
                                    Either::Right(answer) => {
                                        rsx! {
                                            p {"{question}"}
                                            CardRefRender {
                                                selected_card: answer.selected_card(),
                                                placeholder: "pick ittt",
                                                allowed: vec![CardTy::Instance],
                                                filter: answer.filter.clone(),
                                             }
                                        }
                                    },
                                }
                            },
                            AttrAnswer::New {question, mut answer, attr_id} => {
                                rsx! {
                                    match answer.clone().as_ref() {
                                        Some(answer) => {
                                            match answer.clone() {
                                                Either::Left(answer) => {
                                                    rsx! {
                                                        p {"{question}"}
                                                        BackPutRender {
                                                            text: answer.text.clone(),
                                                            dropdown: answer.dropdown.clone(),
                                                            ref_card: answer.ref_card.clone(),
                                                            audio: answer.audio.clone(),
                                                        }
                                                    }

                                                },
                                                Either::Right(answer) => {
                                                    rsx! {
                                                        p {"{question}"}
                                                        CardRefRender {
                                                            selected_card: answer.selected_card(),
                                                            placeholder: "pick ittt",
                                                            allowed: vec![CardTy::Instance],
                                                            filter: answer.filter.clone(),
                                                        }
                                                    }
                                                },
                                            }
                                        }
                                        None => {
                                            rsx! {
                                                div {
                                                    class: "flex flex-row",
                                                    p{"{question}"}
                                                    button {
                                                    class: "mt-2 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base md:mt-0",
                                                    onclick: move |_| {
                                                        match attr_id.back_type {
                                                            Some(id) => {
                                                               let mut cref = CardRef::new();
                                                               cref.filter = SetExpr::union_with([DynCard::Instances(id)]);
                                                               answer.set(Some(Either::Right(cref)))
                                                            },
                                                            None => {
                                                               answer.set(Some(Either::Left(BackPut::new(None))));

                                                            },
                                                        }
                                                    },
                                                    "add answer"
                                                }
                                                }

                                            }
                                        },
                                    }
                                }
                            },
                        }
                    }


            }

                }

            },
        }

        if has_attrs {
            p {"attributes"}
            div {
                class: "max-h-64 overflow-y-auto",
                for (_id, (mut pattern, backty)) in inner_attrs {
                    div {
                    class: "flex flex-row",
                    input {
                        class: "bg-white w-full border border-gray-300 rounded-md p-2 mb-4 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                        value: "{pattern}",
                        placeholder: "default question",
                        oninput: move |evt| pattern.set(evt.value()),
                    }

                    CardRefRender { selected_card: backty.selected_card(), placeholder: "answer type", allowed: vec![CardTy::Class] , filter: speki_core::set::SetExpr::union_with([DynCard::CardType(speki_core::card::CType::Class)])}

                    }
                }
            }
        }

        if is_class {
            button {
                class: "mt-2 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base md:mt-0",
                onclick: move |_| {
                    attrs.write().push((AttributeId::new_v4(), (Signal::new_in_scope("{}".to_string(), ScopeId::APP), CardRef::new())));
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
fn DeleteButton(card: CardId) -> Element {
    rsx! {
        button {
            class: "mt-2 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base md:mt-0",
            onclick: move |_| {
                APP.read().inner().provider.cards.insert_ledger(TheLedgerEvent::new_delete(card)).unwrap();
                pop_overlay();
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
fn AiComplete(CardViewer: CardViewer, api_key: Option<String>) -> Element {
    let selv = CardViewer.clone();

    let class = "mt-2 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base md:mt-0";

    rsx! {
        button {
            class: "{class}",
            onclick: move |_| {
                let front: String = selv.editor.front.text.cloned();
                if front.trim().is_empty() {
                    return;
                }

                let back = selv.editor.back.text.clone();
                let key = api_key.clone();

                spawn(async move {
                    let response = ask_openai(key.clone().unwrap(), front).await;
                    back.clone().set(response);
                });

            },
            "ai complete"
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

                        for event in events {
                            APP.read().inner().provider.cards.insert_ledger(event).unwrap();
                        }

                        for answer in card.answered_attrs {
                            match answer {
                                AttrAnswer::New { attr_id, question: _, answer } => {
                                    if let Some(answer) = answer.cloned() {

                                    match answer {
                                        Either::Left(answer) => {
                                            if let Some(back) = answer.to_backside() {
                                                let data = CardType::Attribute { attribute: attr_id.id, back: back, instance: id };
                                                let action = CardAction::UpsertCard(data);
                                                let event = CardEvent::new(CardId::new_v4(), action);
                                                APP.read().inner().provider.cards.insert_ledger(event).unwrap();
                                            }
                                        },
                                        Either::Right(answer) => {
                                            let card = answer.selected_card().cloned().unwrap();
                                            let back = BackSide::Card(card);
                                            let data = CardType::Attribute { attribute: attr_id.id, back: back, instance: id };
                                            let action = CardAction::UpsertCard(data);
                                            let event = CardEvent::new(CardId::new_v4(), action);
                                            APP.read().inner().provider.cards.insert_ledger(event).unwrap();
                                        },
                                    }
                                    }
                                },
                                AttrAnswer::Old { id: attr_card_id, question: _, answer, attr_id } => {
                                    let prev_back = APP.read().inner().card_provider.providers.cards.load(id).unwrap().ref_backside().cloned().unwrap();
                                    match answer {
                                        Either::Left(answer) => {
                                            if let Some(back) = answer.to_backside() {
                                                if back != prev_back {
                                                    let data = CardType::Attribute { attribute: attr_id, back: back, instance: id };
                                                    let action = CardAction::UpsertCard(data);
                                                    let event = CardEvent::new(attr_card_id, action);
                                                    APP.read().inner().provider.cards.insert_ledger(event).unwrap();
                                                }
                                            }
                                        },
                                        Either::Right(answer) => {
                                            let card = answer.selected_card().cloned().unwrap();
                                            let back = BackSide::Card(card);
                                            let data = CardType::Attribute { attribute: attr_id, back: back, instance: id };
                                            let action = CardAction::UpsertCard(data);
                                            let event = CardEvent::new(attr_card_id, action);
                                            APP.read().inner().provider.cards.insert_ledger(event).unwrap();
                                        },
                                    }
                                },
                            }
                        }

                        let Some(card) = APP.read().inner().card_provider().load(id) else {
                            dbg!(id);
                            panic!();
                        };

                        let inner_card = Arc::unwrap_or_clone(card);
                        let card = Signal::new_in_scope(inner_card.clone(),  ScopeId::APP);
                        if let Some(hook) = selveste.save_hook.clone() {
                            hook.call(card).await;
                        }

                        selveste.reset().await;
                        pop_overlay();
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
    rsx! {
        button {
            class: "mt-2 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base md:mt-0",
            onclick: move |_| {
                let selv = selv.clone();
                let selv2 = selv.clone();

                let fun = MyClosure::new(
                    move |card: Signal<Card>| {
                    selv.editor.dependencies.clone().write().push(card);
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
                let dependent: Node = selv2.tempnode.clone().into();
                let props = CardSelector::dependency_picker(fun).with_dependents(vec![dependent]).with_default_search(thefront.clone());
                append_overlay(OverlayEnum::CardSelector(props));
                info!("2 scope is ! {:?}", current_scope_id().unwrap());
            },
            "add dependency"
        }
    }
}
