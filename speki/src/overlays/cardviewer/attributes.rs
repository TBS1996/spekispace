use std::{collections::BTreeSet, sync::Arc};

use dioxus::prelude::*;
use ledgerstore::{PropertyCache, TheCacheGetter, TheLedgerEvent};
use speki_core::{
    card::{AttrBackType, AttributeId, Attrv2, CType, CardId, RawCard},
    collection::DynCard,
    ledger::CardAction,
    set::SetExpr,
    Card, CardProperty,
};

use crate::{
    components::{
        backside::{bool_editor, opt_bool_editor, BackPutRender, TimestampRender},
        cardref::{CardRefRender, ForcedCardRefRender},
        dropdown::{ActionDropdown, DropdownAction},
        BackPut, CardTy, DeleteButton, SectionWithTitle,
    },
    overlays::{
        card_selector::{CardSelector, MyClosure},
        OverlayEnum,
    },
    utils::handle_card_event_error,
    APP,
};

#[derive(Clone, Debug)]
pub struct AttrEditor {
    pub id: AttributeId,
    pub pattern: Signal<String>,
    pub ty: Signal<Option<AttrBackTypeEditor>>,
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

impl From<Attrv2> for AttrEditor {
    fn from(attr: Attrv2) -> Self {
        let ty: Option<AttrBackTypeEditor> = attr.back_type.map(From::from);

        AttrEditor {
            id: attr.id,
            pattern: Signal::new_in_scope(attr.pattern, ScopeId::APP),
            ty: Signal::new_in_scope(ty, ScopeId::APP),
        }
    }
}

#[derive(Clone, Debug)]
pub enum AttrBackTypeEditor {
    InstanceOfClass(Signal<CardId>),
    Timestamp,
    Boolean,
}

impl From<AttrBackType> for AttrBackTypeEditor {
    fn from(value: AttrBackType) -> Self {
        match value {
            AttrBackType::Boolean => AttrBackTypeEditor::Boolean,
            AttrBackType::InstanceOfClass(id) => {
                AttrBackTypeEditor::InstanceOfClass(Signal::new_in_scope(id, ScopeId::APP))
            }
            AttrBackType::TimeStamp => AttrBackTypeEditor::Timestamp,
        }
    }
}

#[component]
pub fn RenderAttrs(
    card: Option<CardId>,
    attrs: Signal<Vec<AttrEditor>>,
    inherited: bool,
) -> Element {
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

    let children = rsx! {
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
                                    dbg!();
                                    let event: TheLedgerEvent<RawCard> = TheLedgerEvent::new_modify(card, CardAction::RemoveAttr(id));
                                    if let Err(e) = APP.read().inner().provider.cards.modify(event) {
                                        handle_card_event_error(e);
                                        return;
                                    }
                                    attrs.clone().set(load_attr_editors(card));
                                },
                                None => {
                                    dbg!();
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
                            Some(AttrBackTypeEditor::Boolean) => rsx!{
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
                                    "boolean"
                                }

                            },
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
                                let boolean = DropdownAction::new("boolean".to_string(), Box::new(move || {ty.clone().set(Some(AttrBackTypeEditor::Boolean));})).with_title("answer must be boolean");
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
                                        ActionDropdown { label: "set answer constraint".to_string(), options: vec![timestamp, instance, boolean], title: "hey"  }
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
    };

    rsx! {
        div {
            if inherited {
                SectionWithTitle {
                    title: "Inherited attributes".to_string(),
                    tooltip: "attributes inherited from parent classes",
                    children
                }
            } else {
                SectionWithTitle {
                    title: "Attributes".to_string(),
                    on_add: move |_| {
                        attrs.write().push(AttrEditor::new());
                    },
                    children
                }
            }
        }
    }
}

pub fn load_attr_editors(card_id: CardId) -> Vec<AttrEditor> {
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

pub fn load_attr_qa_for_class(card: CardId) -> Vec<AttrQandA> {
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

pub fn load_attr_qa(card: CardId) -> Vec<AttrQandA> {
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
            Some(AttrBackType::Boolean) => {
                let b = card
                    .back_side()
                    .and_then(|ts| ts.as_bool())
                    .unwrap_or_default();
                OldAttrAnswerEditor::Boolean(Signal::new_in_scope(b, ScopeId::APP))
            }
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

/*

idea:

when selecting instannce

it should come up all the attributes from the parent classes (recursively), like it'll ask the asnwer to those questions
and if you answer it it'll create those attr cards

like if `person` has attribute when was {} born, where was {} born,
then when you add a new person instance it'll have those textfields for those questions so that you can create them easilyy that way


*/

pub fn load_inherited_attr_editors(card_id: CardId) -> Vec<AttrEditor> {
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

#[component]
fn OldAttrAnswerEditorRender(answer: OldAttrAnswerEditor) -> Element {
    match answer {
        OldAttrAnswerEditor::Any(answer) => {
            rsx! {
                BackPutRender {
                    text: answer.text.clone(),
                    dropdown: answer.dropdown.clone(),
                    ref_card: answer.ref_card.clone(),
                    boolean: answer.boolean.clone(),
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
        OldAttrAnswerEditor::Boolean(boolean) => rsx! {bool_editor { boolean }},
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
                    boolean: answer.boolean.clone(),
                }
            }
        }
        AttrAnswerEditor::Boolean(boolean) => rsx! {opt_bool_editor { boolean }},
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
pub fn AttrAnswers(
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
                                        show_deps: true,
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
                                                    Some(AttrBackType::Boolean) => {
                                                        answer.set(Some(AttrAnswerEditor::Boolean(Signal::new_in_scope(None, ScopeId::APP))))
                                                    },
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

#[derive(Clone, Debug, PartialEq)]
pub enum OldAttrAnswerEditor {
    Any(BackPut),
    Card {
        filter: SetExpr,
        selected: Signal<CardId>,
    },
    TimeStamp(Signal<String>),
    Boolean(Signal<bool>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum AttrAnswerEditor {
    Any(BackPut),
    Card {
        filter: SetExpr,
        instance_of: Option<CardId>,
        selected: Signal<Option<CardId>>,
    },
    TimeStamp(Signal<String>),
    Boolean(Signal<Option<bool>>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum AttrQandA {
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
    pub fn is_same(me: Vec<Self>, them: Vec<Self>) -> bool {
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
