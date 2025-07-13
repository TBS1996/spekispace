use std::{
    collections::{BTreeSet, HashMap},
    mem,
    str::FromStr,
    sync::Arc,
};

use dioxus::prelude::*;
use ledgerstore::{PropertyCache, TheCacheGetter, TheLedgerEvent};
use omtrent::TimeStamp;
use speki_core::{
    card::{AttrBackType, AttributeId, Attrv2, BackSide, CType, CardId, RawCard, TextData},
    collection::DynCard,
    ledger::{CardAction, CardEvent, MetaAction, MetaEvent},
    metadata::Metadata,
    set::SetExpr,
    Card, CardProperty, CardType,
};
use tracing::info;

use crate::{
    components::{
        backside::{BackPutRender, BacksideError, TimestampRender},
        card_mastery::MasterySection,
        cardref::{CardRefRender, ForcedCardRefRender, OtherCardRefRender},
        dropdown::{ActionDropdown, DropComponent, DropdownAction},
        frontside::FrontPutRender,
        BackPut, CardTy, DropDownMenu, FrontPut, RenderDependents,
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

    let ty = viewer.editor.front.dropdown.selected.clone();
    let card_id = viewer.old_card.as_ref().map(|c| c.id());

    rsx! {
        InputElements {
            front: viewer.editor.front.clone(),
            back: viewer.editor.back.clone(),
            default_question: viewer.editor.default_question.clone(),
            concept: viewer.editor.concept.clone(),
            ty: ty.cloned(),
            card_id,
            namespace: viewer.editor.namespace.clone(),
            attrs: viewer.editor.attrs.clone(),
            inherited_attrs: viewer.editor.inherited_attrs.clone(),
            attr_answers: viewer.editor.attr_answers.clone(),
            fixed_concept: viewer.editor.fixed_concept,
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

    let properties_width = 600;
    let mastery_min_width = 200;
    let _mastery_max_width = 300;

    let wide_screen = *width.read() > properties_width + mastery_min_width;

    let card_class = if wide_screen {
        "w-[600px] min-w-[600px] flex-shrink-0"
    } else {
        "max-w-[600px] w-full flex-shrink"
    };

    rsx! {
        div {
            class: "flex flex-row mx-auto min-w-0",
            style: "max-width: 100%;",

            div {
                class: "p-2 box-border {card_class}",
                CardProperties { viewer: props.clone() }

                if !wide_screen {
                    DisplayMetadata { metadata: props.editor.metadata.clone()  }
                }

                div {
                    class: "flex flex-row mt-4 gap-x-4",

                    save_button { CardViewer: props.clone() }

                    div {
                        if let Some(card) = props.old_card.clone() {
                            DeleteButton{card_id: card.id()}
                        }
                    }
                }
            }

            div {
                class: "flex flex-col",
                if wide_screen {
                    DisplayMetadata { metadata: props.editor.metadata.clone()  }

                    if let Some(history) = history {
                        div {
                            class: "min-w-[200px] max-w-[300px] w-full flex-shrink",
                            MasterySection { history }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct MetadataEditor {
    suspended: Signal<bool>,
    trivial: Signal<bool>,
}

impl MetadataEditor {
    fn new() -> Self {
        Self {
            suspended: Signal::new_in_scope(false, ScopeId::APP),
            trivial: Signal::new_in_scope(false, ScopeId::APP),
        }
    }
}

impl From<Metadata> for MetadataEditor {
    fn from(value: Metadata) -> Self {
        let Metadata {
            trivial,
            suspended,
            id: _,
        } = value;

        Self {
            suspended: Signal::new_in_scope(suspended.is_suspended(), ScopeId::APP),
            trivial: Signal::new_in_scope(trivial.unwrap_or_default(), ScopeId::APP),
        }
    }
}

#[component]
fn DisplayMetadata(metadata: MetadataEditor) -> Element {
    let MetadataEditor { suspended, trivial } = metadata;

    rsx! {
        h3 {
            class: "text-sm font-semibold text-gray-600 uppercase tracking-wide",
            "Metadata"
        }

        div {
            class: "flex flex-row items-center mb-4",
            div {
                class: "w-24",
                p {
                    title: "trivial cards are not reviewed",
                    "trivial"
                }
            }
            DropComponent { options: vec![false, true], selected: trivial }
        }

        div {
            class: "flex flex-row items-center mb-4",
            div {
                class: "w-24",
                p {
                    title: "trivial cards are not reviewed",
                    "suspended"
                }
            }

            DropComponent { options: vec![false, true], selected: suspended }
        }
    }
}

/// All th einfo needed to create the actual card, similar to the cardeditor struct
/// except that this is guaranteed to have all the info needed to create a card
/// while the careditor can always be in an unfinished state
pub struct CardRep {
    ty: CardType,
    namespace: Option<CardId>,
    deps: Vec<CardId>,
    answered_attrs: Vec<AttrQandA>,
    meta: MetadataEditor,
}

#[derive(Clone, Debug, PartialEq)]
enum OldAttrAnswerEditor {
    Any(BackPut),
    Card {
        filter: SetExpr,
        selected: Signal<CardId>,
    },
    TimeStamp(Signal<String>),
}

#[derive(Clone, Debug, PartialEq)]
enum AttrAnswerEditor {
    Any(BackPut),
    Card {
        filter: SetExpr,
        instance_of: Option<CardId>,
        selected: Signal<Option<CardId>>,
    },
    TimeStamp(Signal<String>),
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

impl AttrQandA {
    fn is_same(me: Vec<Self>, them: Vec<Self>) -> bool {
        if me.len() != them.len() {
            return false;
        }

        for attr in me {
            if !them.iter().any(|a| match (&attr, a) {
                (
                    AttrQandA::Old {
                        id: me_id,
                        attr_id: me_attr_id,
                        ..
                    },
                    AttrQandA::Old { id, attr_id, .. },
                ) => me_id == id && me_attr_id == attr_id,
                (AttrQandA::Old { .. }, AttrQandA::New { .. }) => false,
                (AttrQandA::New { .. }, AttrQandA::Old { .. }) => false,
                (AttrQandA::New { attr_id: me_id, .. }, AttrQandA::New { attr_id, .. }) => {
                    me_id == attr_id
                }
            }) {
                return false;
            }
        }

        true
    }
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
    Timestamp,
}

impl From<AttrBackType> for AttrBackTypeEditor {
    fn from(value: AttrBackType) -> Self {
        match value {
            AttrBackType::InstanceOfClass(id) => {
                AttrBackTypeEditor::InstanceOfClass(Signal::new_in_scope(id, ScopeId::APP))
            }
            AttrBackType::TimeStamp => AttrBackTypeEditor::Timestamp,
        }
    }
}

/// container for all the structs you edit while creating/modifying a card
#[derive(Props, Clone, Debug)]
pub struct CardEditor {
    pub front: FrontPut,
    namespace: Signal<Option<CardId>>,
    back: BackPut,
    default_question: Signal<String>,
    concept: Signal<Option<CardId>>,
    dependencies: Signal<Vec<CardId>>,
    allowed_cards: Vec<CardTy>,
    attrs: Signal<Vec<AttrEditor>>,
    inherited_attrs: Signal<Vec<AttrEditor>>,
    attr_answers: Signal<Vec<AttrQandA>>,
    fixed_concept: bool,
    metadata: MetadataEditor,
}

impl CardEditor {
    fn into_cardrep(self) -> Result<CardRep, String> {
        let backside = self.back.clone();
        let frontside = self.front.clone();

        let front = format!("{}", frontside.text.cloned());

        if front.is_empty() {
            return Err("front side can't be empty".to_string());
        }

        for attr_qa in self.attr_answers.cloned() {
            match attr_qa {
                AttrQandA::Old {
                    answer: OldAttrAnswerEditor::TimeStamp(ts),
                    ..
                } => {
                    if let Err(_) = TimeStamp::from_str(&ts.cloned()) {
                        return Err("invalid timestamp".to_string());
                    }
                }
                AttrQandA::New { answer, .. } => {
                    if let Some(ans) = answer.cloned() {
                        if let AttrAnswerEditor::TimeStamp(ts) = ans {
                            if let Err(_) = TimeStamp::from_str(&ts.cloned()) {
                                return Err("invalid timestamp".to_string());
                            }
                        }
                    }
                }
                _ => continue,
            }
        }

        let attrs: HashMap<AttributeId, (String, Option<AttrBackType>)> = self
            .attrs
            .cloned()
            .into_iter()
            .filter_map(|AttrEditor { id, pattern, ty }| {
                let pattern = pattern.cloned();
                let ty = match ty.cloned() {
                    Some(AttrBackTypeEditor::Timestamp) => Some(AttrBackType::TimeStamp),
                    Some(AttrBackTypeEditor::InstanceOfClass(id)) => {
                        Some(AttrBackType::InstanceOfClass(id.cloned()))
                    }
                    None => None,
                };

                Some((id, (pattern, ty)))
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
                let parent_class = self.concept.cloned();
                let back = match backside.try_to_backside() {
                    Ok(back) => Some(back),
                    Err(BacksideError::MissingCard) => None,
                    Err(BacksideError::MissingText) => None,
                    Err(BacksideError::InvalidTimestamp) => {
                        return Err("invalid timestamp".to_string())
                    }
                };

                let attrs: BTreeSet<Attrv2> = attrs
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
                let class = match self.concept.cloned() {
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
            namespace: self.namespace.cloned(),
            deps: self.dependencies.cloned().into_iter().collect(),
            meta: self.metadata,
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
    dependencies: Signal<Vec<CardId>>,
) -> Element {
    let name_and_id: Vec<(String, CardId)> = dependencies
        .cloned()
        .into_iter()
        .map(|card| {
            (
                APP.read()
                    .try_load_card(card)
                    .map(|card| card.name().to_string())
                    .unwrap_or("<deleted card>".to_string()),
                card,
            )
        })
        .collect();

    rsx! {
        div {
            class: "flex flex-col opacity-100 visible w-full h-auto bg-white p-2 shadow-md rounded-md overflow-y-auto",

            div {
                class: "flex items-center justify-between mb-2",

                AdderHeader {
                    title: "Explicit dependencies",
                    on_add: move |_|{
                        let currcard = card_text.cloned();
                        let depsig = dependencies.clone();

                        let fun = MyClosure::new(move |card: CardId| {
                            depsig.clone().write().push(card);
                        });

                        let front = currcard.clone();
                        let mut props = CardSelector::dependency_picker(fun).with_default_search(front);
                        if let Some(id)  = card_id {
                            props = props.with_forbidden_cards(vec![id]);
                        }
                        OverlayEnum::CardSelector(props).append();
                    }
                }
            }

            for (idx, (name, id)) in name_and_id.into_iter().enumerate() {
                div {
                    class: "flex flex-row",
                    button {
                        class: "mb-1 p-1 bg-gray-100 rounded-md text-left",
                        onclick: move|_|{
                            if let Some(card) = APP.read().try_load_card(id) {
                                let viewer = CardViewer::new_from_card(card);
                                OverlayEnum::CardViewer(viewer).append();
                            }
                        },
                        "{name}"
                    }

                    button {
                        class: "p-1 hover:bg-gray-200 hover:border-gray-400 border border-transparent rounded-md transition-colors",
                        onclick: move |_|{
                            let removed =  dependencies.write().remove(idx);
                            if let Some(id) = card_id {
                                let event = TheLedgerEvent::new_modify(id, CardAction::RemoveDependency(removed));
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

fn load_inherited_attr_editors(card_id: CardId) -> Vec<AttrEditor> {
    let Some(card) = APP.read().try_load_card(card_id) else {
        return vec![];
    };

    if !card.is_class() {
        return vec![];
    }

    let attrs = card.attributes().unwrap();

    let mut out: Vec<AttrEditor> = Default::default();

    for attr in attrs {
        let ty: Option<AttrBackTypeEditor> = attr.back_type.map(From::from);

        let editor = AttrEditor {
            id: attr.id,
            pattern: Signal::new_in_scope(attr.pattern, ScopeId::APP),
            ty: Signal::new_in_scope(ty, ScopeId::APP),
        };

        out.push(editor);
    }

    let current: BTreeSet<AttributeId> = load_attr_editors(card_id)
        .into_iter()
        .map(|attr| attr.id)
        .collect();

    out.retain(|attr| !current.contains(&attr.id));

    out
}

fn load_attr_editors(card_id: CardId) -> Vec<AttrEditor> {
    let Some(card) = APP.read().try_load_card(card_id) else {
        return vec![];
    };

    if !card.is_class() {
        return vec![];
    }

    let attrs = card.attributes_on_class().unwrap();

    let mut out: Vec<AttrEditor> = Default::default();

    for attr in attrs {
        let ty: Option<AttrBackTypeEditor> = attr.back_type.map(From::from);

        let editor = AttrEditor {
            id: attr.id,
            pattern: Signal::new_in_scope(attr.pattern, ScopeId::APP),
            ty: Signal::new_in_scope(ty, ScopeId::APP),
        };

        out.push(editor);
    }
    out
}

fn load_attr_qa_for_class(card: CardId) -> Vec<AttrQandA> {
    let Some(card) = APP.read().try_load_card(card) else {
        debug_assert!(false);
        return vec![];
    };

    if !card.is_class() {
        return vec![];
    }

    let curr_card = card.clone();

    let attrs: Vec<Attrv2> = curr_card.attributes().unwrap_or_default();

    let mut output: Vec<AttrQandA> = vec![];

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
            Some(AttrBackType::TimeStamp) => {
                let ts = card
                    .back_side()
                    .and_then(|ts| ts.as_timestamp())
                    .map(|ts| ts.serialize())
                    .unwrap_or_default();

                OldAttrAnswerEditor::TimeStamp(Signal::new_in_scope(ts, ScopeId::APP))
            }
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

        let mut question = attr.pattern.clone();
        if question.contains("{}") {
            question = card.front_side().to_string()
        }

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

#[derive(Props, Clone, Debug)]
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

    pub fn with_class(mut self, class: CardId) -> Self {
        self.editor.concept.clone().set(Some(class));
        self.editor.fixed_concept = true;
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

        let card_id = card.id();

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
            let concept = Signal::new_in_scope(raw_ty.data.class(), ScopeId::APP);

            let attr_answers = { Signal::new_in_scope(load_attr_qa(card_id), ScopeId::APP) };

            let attrs = load_attr_editors(card.id());
            let inherited_attrs = load_inherited_attr_editors(card.id());

            let namespace = {
                if let Some(card) = card.namespace() {
                    Signal::new_in_scope(Some(card), ScopeId::APP)
                } else {
                    Signal::new_in_scope(None, ScopeId::APP)
                }
            };

            let dependencies: Signal<Vec<CardId>> = Signal::new_in_scope(
                card.explicit_dependencies().into_iter().collect(),
                ScopeId(3),
            );

            let default_question = if let CardType::Class {
                default_question, ..
            } = card.clone_base().data
            {
                default_question.unwrap_or_default().to_raw()
            } else {
                String::new()
            };

            let metadata = MetadataEditor::from(
                APP.read()
                    .inner()
                    .provider
                    .metadata
                    .try_load(card.id())
                    .unwrap_or_default(),
            );

            CardEditor {
                front,
                attrs: Signal::new_in_scope(attrs, ScopeId::APP),
                inherited_attrs: Signal::new_in_scope(inherited_attrs, ScopeId::APP),
                attr_answers,
                namespace,
                back,
                concept,
                dependencies,
                allowed_cards: vec![],
                default_question: Signal::new_in_scope(default_question, ScopeId::APP),
                fixed_concept: false,
                metadata,
            }
        };

        Self {
            editor,
            old_card: Some(card.clone()),
            save_hook: None,
        }
    }

    pub fn new() -> Self {
        let concept = Signal::new_in_scope(None, ScopeId::APP);
        let attr_answers: Signal<Vec<AttrQandA>> = Signal::new_in_scope(vec![], ScopeId::APP);
        let front = FrontPut::new(CardTy::Normal);
        let dependencies: Signal<Vec<CardId>> =
            Signal::new_in_scope(Default::default(), ScopeId::APP);

        let editor = {
            let back = BackPut::new(None);

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
                inherited_attrs: Signal::new_in_scope(Default::default(), ScopeId::APP),
                fixed_concept: false,
                metadata: MetadataEditor::new(),
            }
        };

        Self {
            editor,
            old_card: None,
            save_hook: None,
        }
    }

    pub fn with_dependency(mut self, dep: CardId) -> Self {
        self.editor.dependencies.push(dep);
        self
    }

    fn reset(&self) {
        self.editor.front.reset();
        self.editor.back.reset();
        self.editor.dependencies.clone().write().clear();
        self.editor.attrs.clone().write().clear();
        self.editor.attr_answers.clone().write().clear();
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
        OldAttrAnswerEditor::TimeStamp(text) => {
            rsx! {
                TimestampRender { text }
            }
        }
    }
}

#[component]
fn AttrAnswerEditorRender(answer: AttrAnswerEditor) -> Element {
    match answer {
        AttrAnswerEditor::TimeStamp(text) => {
            rsx! {
                TimestampRender { text }
            }
        }
        AttrAnswerEditor::Any(answer) => {
            rsx! {
                BackPutRender {
                    text: answer.text.clone(),
                    dropdown: answer.dropdown.clone(),
                    ref_card: answer.ref_card.clone(),
                }
            }
        }
        AttrAnswerEditor::Card {
            filter,
            selected,
            instance_of,
        } => {
            rsx! {
                CardRefRender {
                    selected_card: selected,
                    placeholder: "select card",
                    allowed: vec![CardTy::Instance],
                    filter,
                    instance_of,
                }
            }
        }
    }
}

#[component]
fn AttrAnswers(
    card: Option<CardId>,
    attr_answers: Signal<Vec<AttrQandA>>,
    class: Signal<Option<CardId>>,
) -> Element {
    rsx! {
        h4 {
            class: "font-bold",
            p { "Attributes" }
        }

        div {
            class: "max-h-64 overflow-y-auto flex flex-col gap-2 min-h-[7rem]",

            for answer in attr_answers.iter() {
                match answer.clone() {
                    AttrQandA::Old { question, answer, id, .. } => {
                        rsx! {
                            div {
                                class: "border border-black p-3 rounded flex flex-row items-center gap-2",
                                p {
                                    class: "font-semibold",
                                    "{question}: "
                                }

                                OldAttrAnswerEditorRender { answer }

                                div {
                                    class: "max-h-32",
                                    DeleteButton {
                                        card_id: id,
                                        pop_ol: false,
                                        f: Some(MyClosure::new(move |_card: CardId| {
                                            let inner = class.cloned();
                                            class.clone().set(inner);
                                        })),
                                    }
                                }
                            }
                        }
                    }

                    AttrQandA::New { question, mut answer, attr_id } => {
                        match answer.cloned() {
                            Some(the_answer) => {
                                rsx! {
                                    div {
                                        class: "border border-black p-3 rounded flex flex-row items-center gap-2",
                                        p {
                                            class: "font-semibold",
                                            "{question}: "
                                        }
                                        button {
                                            class: "{crate::styles::DELETE_BUTTON}",
                                            onclick: move |_| {
                                                answer.set(None);
                                            },
                                            "X"
                                        }
                                        AttrAnswerEditorRender { answer: the_answer }
                                    }
                                }
                            }
                            None => {
                                rsx! {
                                    div {
                                        class: "border border-black p-3 rounded flex flex-row items-center gap-2",

                                        div {
                                            class: "w-[20ch] shrink-0",
                                            p {
                                                class: "font-semibold break-words",
                                                "{question}:"
                                            }
                                        }

                                        button {
                                            class: "{crate::styles::CREATE_BUTTON}",
                                            onclick: move |_| {
                                                match attr_id.back_type {
                                                    Some(AttrBackType::TimeStamp) => {
                                                        answer.set(Some(AttrAnswerEditor::TimeStamp(Signal::new_in_scope(String::new(), ScopeId::APP))));
                                                    }
                                                    Some(AttrBackType::InstanceOfClass(id)) => {
                                                        let filter = SetExpr::union_with([DynCard::Instances(id)]);
                                                        let ans = AttrAnswerEditor::Card {
                                                            filter,
                                                            instance_of: Some(id),
                                                            selected: Signal::new_in_scope(None, ScopeId::APP),
                                                        };
                                                        answer.set(Some(ans));
                                                    }
                                                    None => {
                                                        answer.set(Some(AttrAnswerEditor::Any(BackPut::new(None))));
                                                    }
                                                }
                                            },
                                            "add answer"
                                        }
                                    }
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
pub fn AdderHeader(title: &'static str, on_add: EventHandler<()>) -> Element {
    rsx! {
        div {
            class: "flex items-center mb-2",
            h4 {
                class: "font-bold",
                "{title}"
            }
            button {
                class: "ml-4 p-1 hover:bg-gray-200 hover:border-gray-400 border border-transparent rounded-md transition-colors",
                onclick: move |_| on_add.call(()),
                "➕"
            }
        }
    }
}

#[component]
fn RenderAttrs(card: Option<CardId>, attrs: Signal<Vec<AttrEditor>>, inherited: bool) -> Element {
    let foobar: Vec<(AttrEditor, bool, &'static str)> = attrs
        .cloned()
        .into_iter()
        .map(|attr| {
            let getter = TheCacheGetter::Property(PropertyCache::new(
                CardProperty::AttrId,
                attr.id.to_string(),
            ));
            let cached = APP.read().inner().provider.cards.load_getter(getter);
            let disabled = !cached.is_empty();
            let title = if inherited {
                "can't delete inherited attributes"
            } else if disabled {
                "can't delete used attributes"
            } else {
                ""
            };
            (attr, disabled || inherited, title)
        })
        .collect();

    rsx! {
        div {
            class: "flex flex-row items-center",

            if inherited {
                div {
                    class: "flex items-center mb-2",
                    h4 {
                        class: "font-bold",
                        title: "attributes inherited from parent classes",
                        "Inherited attributes"
                    }
                }
            } else {
                AdderHeader {
                    title: "Attributes",
                    on_add: move |_| {
                        attrs.write().push(AttrEditor::new());
                    },
                }
            }
        }

        div {
            class: "max-h-64 overflow-y-auto",
            for (AttrEditor {id, mut pattern,mut ty }, disabled, title) in foobar {
                div {
                    class: "flex flex-row gap-2 mb-4",

                    button {
                        class: "{crate::styles::DELETE_BUTTON}",
                        disabled: "{disabled}",
                        title: "{title}",
                        onclick: move |_| {
                            match card {
                                Some(card) => {
                                    let event: TheLedgerEvent<RawCard> = TheLedgerEvent::new_modify(card, CardAction::RemoveAttr(id));
                                    if let Err(e) = APP.read().inner().provider.cards.modify(event) {
                                        handle_card_event_error(e);
                                        return;
                                    }
                                    attrs.clone().set(load_attr_editors(card));
                                },
                                None => {
                                    let mut _attrs = attrs.cloned();
                                    _attrs.retain(|a|a.id != id);
                                    attrs.clone().set(_attrs);
                                },
                            };
                        },
                        "delete"
                    }
                    div {
                        class: "w-1/2",
                        input {
                            class: "bg-white w-full border border-gray-300 rounded-md p-2 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                            value: "{pattern}",
                            placeholder: "default question",
                            disabled: inherited,
                            oninput: move |evt| pattern.set(evt.value()),
                        }
                    }
                    div {
                        class: "flex flex-row w-1/2 gap-2",

                        match ty.cloned() {
                            Some(AttrBackTypeEditor::Timestamp) => rsx!{
                                if !inherited {
                                    button {
                                        title: "remove constraint",
                                        class: "{crate::styles::UPDATE_BUTTON}",
                                        onclick: move |_| {
                                            ty.set(None);
                                        },
                                        "X"
                                    }
                                }
                                span {
                                    class: "font-semibold self-center",
                                    "timestamp"
                                }
                            },
                            Some(AttrBackTypeEditor::InstanceOfClass(selected)) => rsx! {
                                if !inherited {
                                    button {
                                        title: "remove constraint",
                                        class: "{crate::styles::UPDATE_BUTTON}",
                                        onclick: move |_| {
                                            ty.set(None);
                                        },
                                        "X"
                                    }
                                }
                                ForcedCardRefRender { selected_card: selected, allowed: vec![CardTy::Class], filter: speki_core::set::SetExpr::union_with([DynCard::CardType(speki_core::card::CType::Class)]), disabled: inherited }
                            },
                            None => {

                                let timestamp = DropdownAction::new("timestamp".to_string(), Box::new(move || {ty.clone().set(Some(AttrBackTypeEditor::Timestamp));})).with_title("answer must be timestamp");
                                let instance = DropdownAction::new("instance".to_string(), Box::new(move || {
                                        let fun = MyClosure::new(move |card: CardId| {
                                            ty.clone().set(Some(AttrBackTypeEditor::InstanceOfClass(Signal::new_in_scope(card, ScopeId::APP))));
                                        });

                                        let filter = SetExpr::union_with([DynCard::CardType(CType::Class)]);
                                        let allowed = vec![CardTy::Class];

                                        let props = CardSelector::ref_picker(fun, filter).with_allowed_cards(allowed);
                                        OverlayEnum::CardSelector(props).append();
                                })).with_title("answer must be instance of a given class");


                                if !inherited {
                                    rsx!{
                                        ActionDropdown { label: "set answer constraint".to_string(), options: vec![timestamp, instance], title: "hey"  }
                                    }
                                } else {
                                    rsx!{}
                                }
                            },
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
    concept: Signal<Option<CardId>>,
    ty: CardTy,
    card_id: Option<CardId>,
    namespace: Signal<Option<CardId>>,
    attrs: Signal<Vec<AttrEditor>>,
    inherited_attrs: Signal<Vec<AttrEditor>>,
    attr_answers: Signal<Vec<AttrQandA>>,
    fixed_concept: bool,
) -> Element {
    use_effect(move || match ((front.dropdown.selected)(), concept()) {
        (CardTy::Instance, Some(class)) => {
            let new_attrs = match card_id {
                Some(card) => load_attr_qa(card),
                None => load_attr_qa_for_class(class),
            };
            let old_attrs = attr_answers.cloned();
            if !AttrQandA::is_same(new_attrs.clone(), old_attrs) {
                attr_answers.set(new_attrs);
            }
        }
        (CardTy::Class, class) => {
            match card_id {
                Some(card) => {
                    let mut old = attrs.cloned();
                    let new_attrs = load_attr_editors(card);

                    let new_attr_ids: BTreeSet<AttributeId> =
                        new_attrs.iter().map(|a| a.id).collect();

                    // Retain old ones if their ID is in the new set
                    old.retain(|a| new_attr_ids.contains(&a.id));

                    // Add new ones if their ID wasn't already in old
                    let old_attr_ids: BTreeSet<AttributeId> = old.iter().map(|a| a.id).collect();

                    let mut combined = old;
                    combined.extend(
                        new_attrs
                            .into_iter()
                            .filter(|a| !old_attr_ids.contains(&a.id)),
                    );
                    if old_attr_ids != new_attr_ids {
                        attrs.clone().set(combined);
                    }

                    let new_attrs = load_inherited_attr_editors(card);
                    inherited_attrs.clone().set(new_attrs);
                }
                None => match class {
                    Some(class) => {
                        let mut new_attrs = load_inherited_attr_editors(class);
                        new_attrs.extend(load_attr_editors(class));
                        inherited_attrs.clone().set(new_attrs);
                    }
                    None => {
                        inherited_attrs.clone().set(vec![]);
                    }
                },
            }
        }
        (_, _) => {
            attr_answers.clone().set(vec![]);
            inherited_attrs.clone().set(vec![]);
            attrs.clone().set(vec![]);
        }
    });

    let has_attr_answers = !attr_answers.read().is_empty();
    let has_inherited_attrs = !inherited_attrs.read().is_empty();

    rsx! {
        FrontPutRender { dropdown: front.dropdown.clone(), text: front.text.clone()}

        match ty {
            CardTy::Unfinished => rsx! {},

            CardTy::Normal => rsx! {
                BackPutRender {
                    text: back.text.clone(),
                    dropdown: back.dropdown.clone(),
                    ref_card: back.ref_card.clone(),
                }
            },
            CardTy::Class => rsx! {
                BackPutRender {
                    text: back.text.clone(),
                    dropdown: back.dropdown.clone(),
                    ref_card: back.ref_card.clone(),
                }


                div {
                    class: "flex items-center flex-row text-gray-700 text-sm font-medium mb-2 mt-4",

                    h3 {
                        class: "font-bold",
                        style: "width: 113px;",
                        title: "pick parent class",
                        "Class"
                    }

                    CardRefRender {
                        selected_card: concept,
                        placeholder: "pick parent class",
                        allowed: vec![CardTy::Class],
                        disabled: fixed_concept,
                    }
                }

                RenderAttrs { attrs, inherited: false }
                if has_inherited_attrs {
                    RenderAttrs { attrs: inherited_attrs, inherited: true }
                }
            },
            CardTy::Instance => rsx! {
                BackPutRender {
                    text: back.text.clone(),
                    dropdown: back.dropdown.clone(),
                    ref_card: back.ref_card.clone(),
                }

                div {
                    class: "flex items-center flex-row text-gray-700 text-sm font-medium mb-2 mt-4",

                    h3 {
                        class: "font-bold",
                        style: "width: 113px;",
                        title: "pick class of instance",
                        "Class"
                    }

                    CardRefRender {
                        selected_card: concept,
                        placeholder: "pick class of instance",
                        allowed: vec![CardTy::Class],
                        disabled: fixed_concept,
                    }
                }

                if has_attr_answers {
                    AttrAnswers { card: card_id, attr_answers, class: concept }
                }
            },
        }

        div {
            class: "mt-4",
            OtherCardRefRender{
                selected_card: namespace.clone(),
                placeholder: "namespace",
                remove_title: "clear namespace",
            },
        }
    }
}

#[component]
fn DeleteButton(
    card_id: CardId,
    pop_ol: Option<bool>,
    f: Option<MyClosure>,
    class: Option<&'static str>,
) -> Element {
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

    let class = format!(
        "{} {}",
        crate::styles::DELETE_BUTTON,
        class.unwrap_or_default()
    );

    rsx! {
        button {
            class,
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
            class: "{crate::styles::UPDATE_BUTTON}",
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

    let button_class = if is_new {
        crate::styles::CREATE_BUTTON
    } else {
        crate::styles::UPDATE_BUTTON
    };

    rsx! {
        button {
            class: "{button_class}",
            title:"{title}",
            disabled: !enabled,
            onclick: move |_| {
                let Ok(CardRep { ty, namespace, deps, answered_attrs, meta }) = selv.editor.clone().into_cardrep() else {
                    return;
                };

                let selveste = selv.clone();
                let mut events: Vec<CardAction> = vec![];

                let id = selveste.old_card.clone().map(|card|card.id()).unwrap_or_else(CardId::new_v4);


                {
                    let event = MetaEvent::new_modify(id, MetaAction::Suspend(meta.suspended.cloned()));

                    if let Err(e) = APP.read().inner().provider.metadata.modify(event) {
                        let err = format!("{e:?}");
                        OverlayEnum::new_notice(err).append();
                        return;
                    }

                    let event = MetaEvent::new_modify(id, MetaAction::SetTrivial(Some(meta.trivial.cloned())));

                    if let Err(e) = APP.read().inner().provider.metadata.modify(event) {
                        let err = format!("{e:?}");
                        OverlayEnum::new_notice(err).append();
                        return;
                    }
                }


                let old_ty = selv.old_card.clone().map(|c|c.clone_base().data);

                let same_type = match &old_ty {
                    Some(old_ty) => {
                        mem::discriminant(old_ty) == mem::discriminant(&ty)
                    },
                    None => false,
                };

                match (ty, same_type) {
                    (CardType::Instance { name, back, class }, false) => {
                        events.push(CardAction::InstanceType { front: name, class });
                        events.push(CardAction::SetBackside(back));
                    }
                    (CardType::Instance { name, back, class }, true) => {
                        events.push(CardAction::SetFront(name));
                        events.push(CardAction::SetBackside(back));
                        events.push(CardAction::SetInstanceClass(class));
                    },
                    (CardType::Normal { front, back }, true) => {
                        events.push(CardAction::SetFront(front));
                        events.push(CardAction::SetBackside(Some(back)));

                    },
                    (CardType::Normal { front, back }, false) => {
                        events.push(CardAction::NormalType {front, back});
                    },
                    (CardType::Unfinished { front }, true) => {
                        events.push(CardAction::SetFront(front));
                    },
                    (CardType::Unfinished { front }, false) => {
                        events.push(CardAction::UnfinishedType {front});
                    },
                    (CardType::Attribute { attribute, back, instance }, true) => {
                        events.push(CardAction::AttributeType {attribute, back, instance});
                    }
                    (CardType::Attribute { attribute, back, instance }, false) => {
                        events.push(CardAction::AttributeType {attribute, back, instance});
                    },
                    (CardType::Class { name, back, parent_class, default_question: _, attrs }, true) => {
                        events.push(CardAction::SetFront(name));
                        events.push(CardAction::SetBackside(back));
                        events.push(CardAction::SetParentClass(parent_class));
                        events.push(CardAction::InsertAttrs(attrs));
                    },
                    (CardType::Class { name, back, parent_class, default_question: _, attrs }, false) => {
                        events.push(CardAction::ClassType { front: name });
                        events.push(CardAction::SetBackside(back));
                        events.push(CardAction::SetParentClass(parent_class));
                        events.push(CardAction::InsertAttrs(attrs));
                    },
                    (CardType::Statement { front }, true) => {
                        events.push(CardAction::SetFront(front));
                    },
                    (CardType::Statement { front }, false) => {
                        events.push(CardAction::StatementType { front });
                    },
                    (CardType::Event {..}, _) => {
                        todo!()
                    },
                }

                events.push(CardAction::SetNamespace ( namespace));
                events.push(CardAction::SetTrivial ( meta.trivial.cloned()));

                for dep in deps {
                    events.push(CardAction::AddDependency(dep));
                }

                for event in events {
                    let event = CardEvent::new_modify(id, event);
                    if let Err(e) = APP.read().inner().provider.cards.modify(event) {
                        handle_card_event_error(e);
                        return;
                    }
                }

                for answer in answered_attrs {
                    match answer {
                        AttrQandA::New { attr_id, question: _, answer } => {
                            if let Some(answer) = answer.cloned() {

                                match answer {
                                    AttrAnswerEditor::TimeStamp(ts) => {
                                        if let Ok(ts) = TimeStamp::from_str(&ts.cloned()) {
                                            let back = BackSide::Time(ts.clone());
                                            let data = CardType::Attribute { attribute: attr_id.id, back: back, instance: id };
                                            let action = CardAction::UpsertCard(data);
                                            let event = CardEvent::new_modify(CardId::new_v4(), action);
                                            if let Err(e) = APP.read().inner().provider.cards.modify(event) {
                                                handle_card_event_error(e);
                                                return;
                                            }
                                        }
                                    },
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
                                    AttrAnswerEditor::Card { filter: _, instance_of: _, selected } => {
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
                                OldAttrAnswerEditor::TimeStamp(ts) => {
                                    let prev_back = APP.read().inner().card_provider.providers.cards.load(attr_card_id).ref_backside().cloned().unwrap();
                                    if let Ok(ts) = TimeStamp::from_str(&ts.cloned()) {
                                        let back = BackSide::Time(ts.clone());
                                        if prev_back != back {
                                            let action = CardAction::SetBackTime(ts);
                                            let event = CardEvent::new_modify(attr_card_id, action);
                                            if let Err(e) = APP.read().inner().provider.cards.modify(event) {
                                                handle_card_event_error(e);
                                                return;
                                            }
                                        }
                                    }
                                },

                                OldAttrAnswerEditor::Any(back_put) => {
                                    let prev_back = APP.read().inner().card_provider.providers.cards.load(attr_card_id).ref_backside().cloned().unwrap();
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
                } else {
                    selveste.reset();
                    pop_overlay();
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
