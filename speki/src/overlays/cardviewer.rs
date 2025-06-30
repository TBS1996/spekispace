use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

use dioxus::prelude::*;
use ledgerstore::TheLedgerEvent;
use speki_core::{
    audio::AudioId,
    card::{AttrBackType, AttributeId, Attrv2, BackSide, CType, CardId, TextData},
    collection::DynCard,
    ledger::{CardAction, CardEvent},
    set::SetExpr,
    Card, CardType,
};
use tracing::info;

use crate::{
    append_overlay,
    components::{
        backside::{BackPutRender, BacksideError},
        card_mastery::MasterySection,
        cardref::{CardRefRender, ForcedCardRefRender},
        frontside::FrontPutRender,
        BackPut, CardRef, CardTy, DropDownMenu, FrontPut, RenderDependents,
    },
    overlays::{
        card_selector::{CardSelector, MyClosure},
        OverlayEnum,
    },
    pop_overlay,
    utils::handle_card_event_error,
    APP,
};

/// The properties of the card itself
#[component]
fn CardProperties(viewer: CardViewer) -> Element {
    let old_card: Option<CardId> = viewer.old_card.as_ref().map(|c| c.id());
    rsx! {
        RenderInputs {
            editor:viewer.editor.clone(),
            save_hook:viewer.save_hook.clone(),
            old_card:viewer.old_card.clone(),
        }
        RenderDependencies { card_text: viewer.editor.front.text.clone(), card_id: old_card, dependencies: viewer.editor.dependencies.clone()}
        if let Some(card_id) = old_card {
            RenderDependents { card_id, hidden: false}
        }
    }
}

#[component]
pub fn CardViewerRender(props: CardViewer) -> Element {
    info!("render cardviewer");

    use dioxus::desktop::use_window;

    let window = use_window();
    let width = use_signal(|| window.inner_size().width);

    use_future(move || {
        to_owned![width, window];
        async move {
            loop {
                let new_width = window.inner_size().width;
                if *width.read() != new_width {
                    width.set(new_width);
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    });

    let history = props
        .old_card
        .as_ref()
        .map(|card| card.history().to_owned());

    // hardcoded for 800px
    let properties_width = 800;
    let mastery_min_width = 200;
    let _mastery_max_width = 300;

    let show_mastery = *width.read() > properties_width + mastery_min_width;

    let card_class = if show_mastery {
        // Tailwind can now detect this
        "w-[800px] min-w-[800px] flex-shrink-0"
    } else {
        "max-w-[800px] w-full flex-shrink"
    };

    rsx! {
        div {
            class: "flex flex-row mx-auto min-w-0",
            style: "max-width: 100%;",

            div {
                class: "p-2 box-border {card_class}",
                CardProperties { viewer: props.clone() }
            }

            if let Some(history) = history {
                if show_mastery {
                    div {
                        class: "min-w-[200px] max-w-[300px] w-full flex-shrink",
                        MasterySection { history }
                    }
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
    answered_attrs: Vec<AttrQandA>,
}

#[derive(Clone, Debug, PartialEq)]
enum OldAttrAnswerEditor {
    Any(BackPut),
    Card {
        filter: SetExpr,
        selected: Signal<CardId>,
    },
}

#[derive(Clone, Debug, PartialEq)]
enum AttrAnswerEditor {
    Any(BackPut),
    Card {
        filter: SetExpr,
        selected: Signal<Option<CardId>>,
    },
}

#[derive(Clone, Debug, PartialEq)]
enum AttrQandA {
    /// There's already a card created for this attribute
    /// answer can be modified but not changed
    Old {
        id: CardId,
        attr_id: AttributeId,
        question: String,
        answer: OldAttrAnswerEditor,
    },
    /// There's not already one, so you can create it.
    /// the id now referes the attribute not the card.
    New {
        attr_id: Attrv2,
        question: String,
        answer: Signal<Option<AttrAnswerEditor>>,
    },
}

#[derive(Clone, Debug)]
struct AttrEditor {
    id: AttributeId,
    pattern: Signal<String>,
    ty: Signal<Option<AttrBackTypeEditor>>,
}

impl AttrEditor {
    fn new() -> Self {
        Self {
            id: AttributeId::new_v4(),
            pattern: Signal::new_in_scope("{}".to_string(), ScopeId::APP),
            ty: Signal::new_in_scope(None, ScopeId::APP),
        }
    }
}

#[derive(Clone, Debug)]
enum AttrBackTypeEditor {
    InstanceOfClass(Signal<CardId>),
}

impl From<AttrBackType> for AttrBackTypeEditor {
    fn from(value: AttrBackType) -> Self {
        match value {
            AttrBackType::InstanceOfClass(id) => {
                AttrBackTypeEditor::InstanceOfClass(Signal::new_in_scope(id, ScopeId::APP))
            }
        }
    }
}

/// container for all the structs you edit while creating/modifying a card
#[derive(Props, Clone)]
pub struct CardEditor {
    pub front: FrontPut,
    namespace: Signal<Option<CardRef>>,
    back: BackPut,
    default_question: Signal<String>,
    concept: CardRef,
    dependencies: Signal<Vec<Arc<Card>>>,
    allowed_cards: Vec<CardTy>,
    attrs: Signal<Vec<AttrEditor>>,
    attr_answers: Signal<Vec<AttrQandA>>,
}

impl CardEditor {
    fn into_cardrep(self) -> Result<CardRep, String> {
        let backside = self.back.clone();
        let frontside = self.front.clone();

        let front = format!("{}", frontside.text.cloned());

        if front.is_empty() {
            return Err("front side can't be empty".to_string());
        }

        let attrs: HashMap<AttributeId, (String, Option<AttrBackType>)> = self
            .attrs
            .cloned()
            .into_iter()
            .filter_map(|AttrEditor { id, pattern, ty }| {
                let pattern = pattern.cloned();
                let ty = match ty.cloned() {
                    Some(AttrBackTypeEditor::InstanceOfClass(id)) => {
                        Some(AttrBackType::InstanceOfClass(id.cloned()))
                    }
                    None => None,
                };
                if pattern.contains("{}") {
                    Some((id, (pattern, ty)))
                } else {
                    None
                }
            })
            .collect();

        let ty = match self.front.dropdown.selected.cloned() {
            CardTy::Normal => {
                let back = match backside.try_to_backside() {
                    Ok(back) => back,
                    Err(BacksideError::MissingCard) => {
                        return Err("no card selected".to_string());
                    }
                    Err(BacksideError::MissingText) => {
                        return Err("backside can't be empty".to_string());
                    }
                    Err(BacksideError::InvalidTimestamp) => {
                        return Err("invalid timestamp".to_string())
                    }
                };

                CardType::Normal {
                    front: TextData::from_raw(&front),
                    back,
                }
            }
            CardTy::Class => {
                let parent_class = self.concept.selected_card().cloned();
                let back = match backside.try_to_backside() {
                    Ok(back) => Some(back),
                    Err(BacksideError::MissingCard) => None,
                    Err(BacksideError::MissingText) => None,
                    Err(BacksideError::InvalidTimestamp) => {
                        return Err("invalid timestamp".to_string())
                    }
                };

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
                let class = match self.concept.selected_card().cloned() {
                    Some(class) => class,
                    None => return Err("must pick class of instance".to_string()),
                };

                let back = match backside.try_to_backside() {
                    Ok(back) => Some(back),
                    Err(BacksideError::MissingCard) => None,
                    Err(BacksideError::MissingText) => None,
                    Err(BacksideError::InvalidTimestamp) => {
                        return Err("invalid timestamp".to_string())
                    }
                };

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

        Ok(CardRep {
            ty,
            answered_attrs: self.attr_answers.cloned(),
            namespace: self
                .namespace
                .cloned()
                .map(|x| x.selected_card().cloned())
                .flatten(),
            front_audio: self.front.audio.cloned().map(|audio| audio.id),
            back_audio: self.back.audio.cloned().map(|audio| audio.id),
            deps: self
                .dependencies
                .cloned()
                .into_iter()
                .map(|c| c.id())
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
    dependencies: Signal<Vec<Arc<Card>>>,
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
                        let depsig = dependencies.clone();

                        let fun = MyClosure::new(move |card: CardId| {
                            let card = APP.read().load_card(card);
                            depsig.clone().write().push(card);
                        });

                        let front = currcard.clone();
                        let mut props = CardSelector::dependency_picker(fun).with_default_search(front);
                        if let Some(id)  = card_id {
                            props = props.with_forbidden_cards(vec![id]);
                        }
                        append_overlay(OverlayEnum::CardSelector(props));
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
                            let viewer = CardViewer::new_from_card(card);
                            append_overlay(OverlayEnum::CardViewer(viewer));
                        },
                        "{card}"
                    }

                    button {
                        class: "p-1 hover:bg-gray-200 hover:border-gray-400 border border-transparent rounded-md transition-colors",
                        onclick: move |_|{
                            let removed =  dependencies.write().remove(idx);
                            if let Some(id) = card_id {
                                let event = TheLedgerEvent::new_modify(id, CardAction::RemoveDependency(removed.id()));
                                if let Err(e) = APP.read().inner().provider.cards.modify(event) {
                                    handle_card_event_error(e);
                                }
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

fn load_attr_qa(card: CardId) -> Vec<AttrQandA> {
    let Some(card) = APP.read().try_load_card(card) else {
        debug_assert!(false);
        return vec![];
    };

    if !card.is_instance() {
        return vec![];
    }

    let curr_card = card.clone();

    let mut attrs: Vec<Attrv2> = curr_card.attributes().unwrap_or_default();

    let provider = APP.read().inner().card_provider.clone();

    // all cards that are an attribute card based on a given instance.
    // wait, isnt this all we need? damn..
    let attr_cards_based_on_instance: BTreeSet<Arc<Card>> = card
        .attribute_cards()
        .into_iter()
        .map(|id| provider.load(id).unwrap())
        .collect();

    attrs.retain(|attr_id| {
        !attr_cards_based_on_instance
            .iter()
            .any(|card| card.uses_attr_id(attr_id.id))
    });

    let mut output: Vec<AttrQandA> = vec![];

    for card in attr_cards_based_on_instance {
        let attr_id = card.attr_id().unwrap();
        let instance = card.attribute_instance();
        let instance = APP.read().inner().card_provider.load(instance).unwrap();
        let attr = instance.get_attr(attr_id).unwrap();

        let answer = match attr.back_type {
            Some(AttrBackType::InstanceOfClass(card_id)) => {
                let filter = SetExpr::union_with([DynCard::Instances(card_id)]);
                let selected = card.back_side().and_then(|bs| bs.as_card()).unwrap();
                OldAttrAnswerEditor::Card {
                    filter,
                    selected: Signal::new_in_scope(selected, ScopeId::APP),
                }
            }
            None => {
                let b = BackPut::new(card.clone_base().data.backside().cloned());
                OldAttrAnswerEditor::Any(b)
            }
        };

        let question = card.front_side().to_string();
        let val = AttrQandA::Old {
            attr_id: card.attr_id().unwrap(),
            id: card.id(),
            question,
            answer,
        };
        output.push(val);
    }

    for attr in attrs {
        let instance = card.name_textdata().to_raw();

        let question = attr.pattern.replace("{}", &instance);

        let val = AttrQandA::New {
            attr_id: attr,
            question,
            answer: Signal::new_in_scope(None, ScopeId::APP),
        };
        output.push(val);
    }

    output
}

#[derive(Props, Clone)]
pub struct CardViewer {
    pub editor: CardEditor,
    pub save_hook: Option<MyClosure>,
    pub old_card: Option<Arc<Card>>,
}

impl PartialEq for CardViewer {
    fn eq(&self, other: &Self) -> bool {
        self.editor == other.editor && self.old_card == other.old_card
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

    pub fn new_from_card(mut card: Arc<Card>) -> Self {
        if card.is_attribute() {
            let instance = card.attribute_instance();
            card = APP.read().load_card(instance);
        }

        let raw_ty = card.clone_base();

        let front = {
            let frnt = FrontPut::new(CardTy::from_ctype(card.card_type()));
            frnt.text.clone().set(raw_ty.data.raw_front());
            frnt
        };

        let back = {
            let bck = BackPut::new(raw_ty.data.backside().cloned());

            let back = match raw_ty.data.backside() {
                Some(BackSide::Time(ts)) => ts.to_string(),
                Some(b) if b.is_text() => b.to_string(),
                _ => String::new(),
            };

            bck.text.clone().set(back);
            bck
        };

        let editor = {
            let concept = {
                let concept = CardRef::new().with_allowed(vec![CardTy::Class]);
                if let Some(class) = raw_ty.data.class() {
                    let class = APP.read().load_card(class);
                    concept.set_ref(class.id());
                }

                concept
            };

            // for instance cards, how you answer certain attributes.

            let attrs = load_attr_qa(card.id());
            let attr_answers = Signal::new_in_scope(attrs, ScopeId::APP);

            // The attributes for a given class
            let attrs: Vec<AttrEditor> = if card.is_class() {
                let attrs = card.attributes().unwrap();

                let mut map: Vec<AttrEditor> = Default::default();

                for attr in attrs {
                    let ty: Option<AttrBackTypeEditor> = attr.back_type.map(From::from);

                    let editor = AttrEditor {
                        id: attr.id,
                        pattern: Signal::new_in_scope(attr.pattern, ScopeId::APP),
                        ty: Signal::new_in_scope(ty, ScopeId::APP),
                    };

                    map.push(editor);
                }
                map
            } else {
                Default::default()
            };

            let namespace = {
                if let Some(card) = card.namespace() {
                    let namespace = CardRef::new();
                    let card = APP.read().load_card(card);
                    namespace.set_ref(card.id());
                    Signal::new_in_scope(Some(namespace), ScopeId::APP)
                } else {
                    Signal::new_in_scope(None, ScopeId::APP)
                }
            };

            let dependencies: Signal<Vec<Arc<Card>>> = Signal::new_in_scope(
                card.explicit_dependencies()
                    .into_iter()
                    .map(|id| APP.read().load_card(id))
                    .collect(),
                ScopeId(3),
            );

            let bck = back;
            let concept = concept;

            let default_question = if let CardType::Class {
                default_question, ..
            } = card.clone_base().data
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
            old_card: Some(card.clone()),
            save_hook: None,
        }
    }

    pub fn new() -> Self {
        let front = FrontPut::new(CardTy::Normal);
        let dependencies: Signal<Vec<Arc<Card>>> =
            Signal::new_in_scope(Default::default(), ScopeId::APP);

        let editor = {
            let back = BackPut::new(None);

            let concept = CardRef::new().with_allowed(vec![CardTy::Class]);

            let attr_answers = Signal::new_in_scope(Default::default(), ScopeId::APP);

            CardEditor {
                attr_answers,
                front,
                namespace: Signal::new_in_scope(None, ScopeId::APP),
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
            old_card: None,
            save_hook: None,
        }
    }

    pub fn with_dependency(mut self, dep: CardId) -> Self {
        let card = APP.read().load_card(dep);
        self.editor.dependencies.push(card);
        self
    }

    fn reset(&self) {
        self.editor.front.reset();
        self.editor.back.reset();
        self.editor.dependencies.clone().write().clear();
    }
}

#[component]
fn RenderInputs(props: CardViewer) -> Element {
    info!("render inputs");
    let ty = props.editor.front.dropdown.selected.clone();
    let card_id = props.old_card.as_ref().map(|c| c.id());
    let card_exists = props.old_card.is_some();

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
            if let Some(card) = props.old_card.clone() {
                if card_exists {
                    DeleteButton{card_id: card.id()}
                }
                Suspend { card: card.id() }
            }

            save_button { CardViewer: props.clone() }
        }
    }
}

#[component]
fn OldAttrAnswerEditorRender(answer: OldAttrAnswerEditor) -> Element {
    match answer {
        OldAttrAnswerEditor::Any(answer) => {
            rsx! {
                BackPutRender {
                    text: answer.text.clone(),
                    dropdown: answer.dropdown.clone(),
                    ref_card: answer.ref_card.clone(),
                    audio: answer.audio.clone(),
                }
            }
        }
        OldAttrAnswerEditor::Card { filter, selected } => {
            rsx! {
                ForcedCardRefRender {
                    selected_card: selected,
                    allowed: vec![CardTy::Instance],
                    filter,
                }
            }
        }
    }
}

#[component]
fn AttrAnswerEditorRender(answer: AttrAnswerEditor) -> Element {
    match answer {
        AttrAnswerEditor::Any(answer) => {
            rsx! {
                BackPutRender {
                    text: answer.text.clone(),
                    dropdown: answer.dropdown.clone(),
                    ref_card: answer.ref_card.clone(),
                    audio: answer.audio.clone(),
                }
            }
        }
        AttrAnswerEditor::Card { filter, selected } => {
            rsx! {
                CardRefRender {
                    selected_card: selected,
                    placeholder: "select card",
                    allowed: vec![CardTy::Instance],
                    filter,
                }
            }
        }
    }
}

#[component]
fn AttrAnswers(card: CardId, attr_answers: Signal<Vec<AttrQandA>>) -> Element {
    rsx! {
        h4 {
            class: "font-bold",
            p {"Attributes"}
        }

        div {
            class: "max-h-64 overflow-y-auto flex flex-col gap-2",

            for answer in attr_answers.iter() {
                match answer.clone() {
                    AttrQandA::Old {question, answer, id, ..} => {
                        rsx! {
                            div {
                                class: "border border-black p-3 rounded flex flex-col gap-2",
                                p { class: "font-semibold", "{question}" }
                                OldAttrAnswerEditorRender { answer }
                                DeleteButton { card_id: id, pop_ol: false, f: {
                                    Some(MyClosure::new(move |_card: CardId|  {
                                        let new_loaded = load_attr_qa(card);
                                        attr_answers.clone().set(new_loaded);
                                    }))
                                } }
                            }
                        }
                    },
                    AttrQandA::New {question, mut answer, attr_id} => {
                        rsx! {
                            match answer.cloned(){
                                Some(the_answer) => {
                                    rsx! {
                                        div {
                                            class: "border border-black p-3 rounded flex flex-col gap-2",
                                            p { class: "font-semibold", "{question}" }

                                            div {
                                                class: "flex flex-row items-start gap-2",
                                                button {
                                                    class: "{crate::styles::BLACK_BUTTON} mt-1",
                                                    onclick: move |_| {
                                                        answer.set(None);
                                                    },
                                                    "X"
                                                }
                                                AttrAnswerEditorRender { answer: the_answer }
                                            }
                                        }
                                    }
                                }

                                None => {
                                    rsx! {
                                        div {
                                            class: "flex flex-row",
                                            p{"{question}"}
                                            button {
                                                class: "{crate::styles::BLACK_BUTTON} ml-4",
                                                onclick: move |_| {
                                                    match attr_id.back_type {
                                                        Some(AttrBackType::InstanceOfClass(id)) => {
                                                            let filter = SetExpr::union_with([DynCard::Instances(id)]);
                                                            let ans = AttrAnswerEditor::Card {
                                                                filter,
                                                                selected: Signal::new_in_scope(None, ScopeId::APP),
                                                            };

                                                            answer.set(Some(ans))
                                                        },
                                                        None => {
                                                            answer.set(Some(AttrAnswerEditor::Any(BackPut::new(None))));
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
}

#[component]
fn RenderAttrs(attrs: Signal<Vec<AttrEditor>>) -> Element {
    rsx! {
        div {
            class: "flex flex-row items-center",

            h4 {
                class: "font-bold",
                "Attributes"
            }

            button {
                class: "ml-4 p-1 hover:bg-gray-200 hover:border-gray-400 border border-transparent rounded-md transition-colors",
                onclick: move |_| {
                    attrs.write().push(AttrEditor::new());
                },
                "➕"
            }
        }

        div {
            class: "max-h-64 overflow-y-auto",
            for AttrEditor {id: _,mut pattern,mut ty } in attrs() {
                div {
                    class: "flex flex-row gap-2 mb-4",
                    div {
                        class: "w-1/2",
                        input {
                            class: "bg-white w-full border border-gray-300 rounded-md p-2 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                            value: "{pattern}",
                            placeholder: "default question",
                            oninput: move |evt| pattern.set(evt.value()),
                        }
                    }
                    div {
                            class: "flex flex-row w-1/2 gap-2",
                        match ty.cloned() {
                            Some(AttrBackTypeEditor::InstanceOfClass(selected)) => rsx! {
                                button {
                                    title: "remove answer constraint",
                                    class: "{crate::styles::BLACK_BUTTON}",
                                    onclick: move |_| {
                                        ty.set(None);
                                    },
                                    "X"
                                }
                                ForcedCardRefRender { selected_card: selected, allowed: vec![CardTy::Class], filter: speki_core::set::SetExpr::union_with([DynCard::CardType(speki_core::card::CType::Class)]) }
                            },
                            None => rsx! {
                                button {
                                    class: "{crate::styles::BLACK_BUTTON}",
                                    onclick: move |_| {
                                        let fun = MyClosure::new(move |card: CardId| {
                                            ty.clone().set(Some(AttrBackTypeEditor::InstanceOfClass(Signal::new_in_scope(card, ScopeId::APP))));
                                        });

                                        let filter = SetExpr::union_with([DynCard::CardType(CType::Class)]);
                                        let allowed = vec![CardTy::Class];

                                        let props = CardSelector::ref_picker(fun, filter).with_allowed_cards(allowed);
                                        append_overlay(OverlayEnum::CardSelector(props));
                                    },
                                    "add instance constraint"
                                }
                            }
                        }
                    }
                }
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
    mut namespace: Signal<Option<CardRef>>,
    attrs: Signal<Vec<AttrEditor>>,
    attr_answers: Signal<Vec<AttrQandA>>,
) -> Element {
    let has_attr_answers = !attr_answers.read().is_empty();

    rsx! {
        FrontPutRender { dropdown: front.dropdown.clone(), text: front.text.clone(), audio: front.audio.clone() }

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
                        allowed: concept.allowed.clone(),
                        filter: concept.filter.clone(),
                    },
                }

                RenderAttrs { attrs }

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
                        allowed: concept.allowed.clone(),
                        filter: concept.filter.clone(),
                    },
                }

                if let (true, Some(card)) = (has_attr_answers, card_id) {
                    AttrAnswers { card, attr_answers }
                }
            },
        }

        match namespace.cloned() {
            Some(ns) => {
                    rsx! {
                        div {
                            class: "block text-gray-700 text-sm font-medium mb-2",
                            style: "margin-right: 82px;",
                        CardRefRender{
                            selected_card: ns.card.clone(),
                            placeholder: "choose namespace",
                            on_select: ns.on_select.clone(),
                            on_deselect: ns.on_deselect.clone(),
                            allowed: ns.allowed.clone(),
                            filter: ns.filter.clone(),
                        },
                    }
                }
            },
            None => {
                rsx! {
                    button {
                        class: "{crate::styles::BLACK_BUTTON} mb-2",
                        onclick: move |_| {
                            namespace.set(Some(CardRef::new()));
                        },
                        "set namespace"
                    }
                }
            },
        }


    }
}

#[component]
fn DeleteButton(card_id: CardId, pop_ol: Option<bool>, f: Option<MyClosure>) -> Element {
    let card = APP.read().inner().card_provider.load(card_id);
    debug_assert!(card.is_some());

    let title: Option<&'static str> = match card {
        Some(card) => {
            if card.dependents_ids().is_empty() {
                None
            } else {
                Some("cannot delete card with dependents")
            }
        }
        None => Some("missing card"),
    };

    let disabled = title.is_some();
    let title = title.unwrap_or_default();
    let pop_ol = pop_ol.unwrap_or(true);

    rsx! {
        button {
            class: "{crate::styles::BLACK_BUTTON}",
            title: "{title}",
            disabled: disabled,
            onclick: move |_| {
                if let Err(e) = APP.read().inner().provider.cards.modify(TheLedgerEvent::new_delete(card_id)) {
                    handle_card_event_error(e);
                    return;
                }
                if let Some(f) = &f {
                    f.call(card_id);
                }

                if pop_ol {
                    pop_overlay();
                }
            },
            "delete"
        }
    }
}

#[component]
fn Suspend(card: CardId) -> Element {
    let mut card = Arc::unwrap_or_clone(APP.read().load_card(card));
    let is_suspended = card.is_suspended();
    let txt = if is_suspended { "unsuspend" } else { "suspend" };

    rsx! {
        button {
            class: "{crate::styles::BLACK_BUTTON}",
            onclick: move |_| {
                card.set_suspend(!is_suspended);
            },
            "{txt}"
        }
    }
}

#[component]
fn save_button(CardViewer: CardViewer) -> Element {
    let selv = CardViewer.clone();

    let is_new = CardViewer.old_card.as_ref().is_none();

    let (enabled, title) = match selv.editor.clone().into_cardrep() {
        Ok(card) => {
            if CardViewer
                .editor
                .allowed_cards
                .contains(&CardTy::from_ctype(card.ty.fieldless()))
                || CardViewer.editor.allowed_cards.is_empty()
            {
                (true, None)
            } else {
                (false, Some("not valid card type".to_string()))
            }
        }
        Err(title) => (false, Some(title)),
    };

    let title = title.unwrap_or_default();

    rsx! {
        button {
            class: "{crate::styles::BLACK_BUTTON}",
            title:"{title}",
            disabled: !enabled,
            onclick: move |_| {
                let Ok(card) = selv.editor.clone().into_cardrep() else {
                    return;
                };

                let selveste = selv.clone();
                let mut events: Vec<CardEvent> = vec![];

                let id = selveste.old_card.clone().map(|card|card.id()).unwrap_or_else(CardId::new_v4);
                events.push(CardEvent::new_modify(id, CardAction::UpsertCard(card.ty)));
                events.push(CardEvent::new_modify(id, CardAction::SetFrontAudio (card.front_audio)));
                events.push(CardEvent::new_modify(id, CardAction::SetBackAudio ( card.back_audio)));
                events.push(CardEvent::new_modify(id, CardAction::SetNamespace ( card.namespace)));

                for dep in card.deps {
                    events.push(CardEvent::new_modify(id, CardAction::AddDependency(dep)));
                }

                for event in events {
                    if let Err(e) = APP.read().inner().provider.cards.modify(event) {
                        handle_card_event_error(e);
                        return;
                    }
                }

                for answer in card.answered_attrs {
                    match answer {
                        AttrQandA::New { attr_id, question: _, answer } => {
                            if let Some(answer) = answer.cloned() {

                                match answer {
                                    AttrAnswerEditor::Any(back_put) => {
                                        if let Some(back) = back_put.to_backside() {
                                            let data = CardType::Attribute { attribute: attr_id.id, back: back, instance: id };
                                            let action = CardAction::UpsertCard(data);
                                            let event = CardEvent::new_modify(CardId::new_v4(), action);
                                            if let Err(e) = APP.read().inner().provider.cards.modify(event) {
                                                handle_card_event_error(e);
                                                return;
                                            }
                                        }
                                    },
                                    AttrAnswerEditor::Card { filter: _, selected } => {
                                        if let Some(card) = selected.cloned() {
                                            let back = BackSide::Card(card);
                                            let data = CardType::Attribute { attribute: attr_id.id, back: back, instance: id };
                                            let action = CardAction::UpsertCard(data);
                                            let event = CardEvent::new_modify(CardId::new_v4(), action);
                                            if let Err(e) = APP.read().inner().provider.cards.modify(event) {
                                                handle_card_event_error(e);
                                                return;
                                            }
                                        }
                                    },
                                }
                            }
                        },
                        AttrQandA::Old { id: attr_card_id, question: _, answer, attr_id } => {
                            match answer {
                                OldAttrAnswerEditor::Any(back_put) => {
                                    let prev_back = APP.read().inner().card_provider.providers.cards.load(id).ref_backside().cloned().unwrap();
                                    if let Some(back) = back_put.to_backside() {
                                        if back != prev_back {
                                            let data = CardType::Attribute { attribute: attr_id, back: back, instance: id };
                                            let action = CardAction::UpsertCard(data);
                                            let event = CardEvent::new_modify(attr_card_id, action);
                                            if let Err(e) = APP.read().inner().provider.cards.modify(event) {
                                                handle_card_event_error(e);
                                                return;
                                            }
                                        }
                                    }

                                },
                                OldAttrAnswerEditor::Card { filter: _, selected } => {
                                    let card = selected.cloned();
                                    let back = BackSide::Card(card);
                                    let data = CardType::Attribute { attribute: attr_id, back: back, instance: id };
                                    let action = CardAction::UpsertCard(data);
                                    let event = CardEvent::new_modify(attr_card_id, action);
                                    if let Err(e) = APP.read().inner().provider.cards.modify(event) {
                                        handle_card_event_error(e);
                                        return;
                                    }
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
                if let Some(hook) = selveste.save_hook.clone() {
                    hook.call(inner_card.id());
                }

                selveste.reset();
                pop_overlay();
            },
            if is_new {
                "create"
            } else {
                "save"
            }
        }
    }
}
