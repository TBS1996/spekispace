use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use dioxus::prelude::*;
use ledgerstore::LedgerEvent;
use speki_core::{
    card::{AttrBackType, AttributeId, Attrv2, BackSide, CType, CardId, ParamAnswer, RawCard},
    collection::DynCard,
    ledger::CardAction,
    set::SetExpr,
    Card,
};

use crate::{
    components::{
        backside::{
            bool_editor, opt_bool_editor, BackPutRender, ForcedTimestampRender, TimestampRender,
        },
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

#[derive(Clone, Debug, PartialEq, Eq)]
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

impl From<AttrBackTypeEditor> for AttrBackType {
    fn from(editor: AttrBackTypeEditor) -> Self {
        match editor {
            AttrBackTypeEditor::InstanceOfClass(class) => {
                AttrBackType::InstanceOfClass(class.cloned())
            }
            AttrBackTypeEditor::Timestamp => AttrBackType::TimeStamp,
            AttrBackTypeEditor::Boolean => AttrBackType::Boolean,
        }
    }
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
pub fn RenderInheritedAttrs(
    card: Option<CardId>,
    attrs: Memo<Vec<AttrEditor>>,
    #[props(default = false)] is_param: bool,
) -> Element {
    let foobar: Vec<(AttrEditor, bool, &'static str)> = attrs
        .cloned()
        .into_iter()
        .map(|attr| {
            let cached = APP.read().load_attrs(attr.id);
            let disabled = !cached.is_empty();

            let title = if is_param {
                "can't delete inherited params"
            } else {
                "can't delete inherited attributes"
            };

            (attr, disabled, title)
        })
        .collect();

    let children = rsx! {
        div {
            class: "max-h-64 overflow-y-auto",
            for (AttrEditor {id: _, mut pattern, ty }, _disabled, _title) in foobar {
                div {
                    class: "flex flex-row gap-2 mb-4",
                    div {
                        class: "w-1/2",
                        input {
                            class: "bg-white w-full border border-gray-300 rounded-md p-2 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                            value: "{pattern}",
                            placeholder: "default question",
                            disabled: true,
                            oninput: move |evt| pattern.set(evt.value()),
                        }
                    }
                    div {
                        class: "flex flex-row w-1/2 gap-2",

                        match ty.cloned() {
                            Some(AttrBackTypeEditor::Boolean) => rsx!{
                                span {
                                    class: "font-semibold self-center",
                                    "boolean"
                                }

                            },
                            Some(AttrBackTypeEditor::Timestamp) => rsx!{
                                span {
                                    class: "font-semibold self-center",
                                    "timestamp"
                                }
                            },
                            Some(AttrBackTypeEditor::InstanceOfClass(selected)) => rsx! {
                                ForcedCardRefRender { selected_card: selected, allowed: vec![CardTy::Class], filter: speki_core::set::SetExpr::union_with([DynCard::CardType(speki_core::card::CType::Class)]), disabled: true }
                            },
                            None => {

                                rsx!{}

                            },
                        }
                    }
                }
            }
        }
    };

    let title = match is_param {
        true => "Inherited Params",
        false => "Inherited attributes",
    };

    rsx! {
        div {
            SectionWithTitle {
                title: title.to_string(),
                tooltip: "attributes inherited from parent classes",
                children
            }
        }
    }
}

#[component]
pub fn RenderAttrs(
    card: Option<CardId>,
    attrs: Signal<Vec<AttrEditor>>,
    #[props(default = false)] is_param: bool,
    #[props(default = false)] disabled: bool,
) -> Element {
    let foobar: Vec<(AttrEditor, bool, &'static str)> = attrs
        .cloned()
        .into_iter()
        .map(|attr| {
            let cached = APP.read().load_attrs(attr.id);
            let disabled = !cached.is_empty() || disabled;

            let title = match (is_param, disabled) {
                (true, true) => "can't delete used params",
                (false, true) => "can't delete used attributes",
                (_, _) => "",
            };

            (attr, disabled, title)
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
                                    if is_param {
                                        let event: LedgerEvent<RawCard> = LedgerEvent::new_modify(card, CardAction::RemoveParam(id));
                                        if let Err(e) = APP.read().modify_card(event) {
                                            handle_card_event_error(e);
                                            return;
                                        }
                                        attrs.clone().set(load_param_editors(card));
                                    } else {
                                    let event: LedgerEvent<RawCard> = LedgerEvent::new_modify(card, CardAction::RemoveAttr(id));
                                    if let Err(e) = APP.read().modify_card(event) {
                                        handle_card_event_error(e);
                                        return;
                                    }
                                    attrs.clone().set(load_attr_editors(card));
                                    }
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
                            disabled,
                            placeholder: "default question",
                            oninput: move |evt| pattern.set(evt.value()),
                        }
                    }
                    div {
                        class: "flex flex-row w-1/2 gap-2",

                        match ty.cloned() {
                            Some(AttrBackTypeEditor::Boolean) => rsx!{
                                button {
                                    title: "remove constraint",
                                    class: "{crate::styles::UPDATE_BUTTON}",
                                    disabled,
                                    onclick: move |_| {
                                        ty.set(None);
                                    },
                                    "X"
                                }
                                span {
                                    class: "font-semibold self-center",
                                    "boolean"
                                }

                            },
                            Some(AttrBackTypeEditor::Timestamp) => rsx!{
                                    button {
                                        title: "remove constraint",
                                        class: "{crate::styles::UPDATE_BUTTON}",
                                        disabled,
                                        onclick: move |_| {
                                            ty.set(None);
                                        },
                                        "X"
                                    }
                                span {
                                    class: "font-semibold self-center",
                                    "timestamp"
                                }
                            },
                            Some(AttrBackTypeEditor::InstanceOfClass(selected)) => rsx! {
                                    button {
                                        title: "remove constraint",
                                        class: "{crate::styles::UPDATE_BUTTON}",
                                        disabled,
                                        onclick: move |_| {
                                            ty.set(None);
                                        },
                                        "X"
                                    }
                                ForcedCardRefRender { disabled, selected_card: selected, allowed: vec![CardTy::Class], filter: speki_core::set::SetExpr::union_with([DynCard::CardType(speki_core::card::CType::Class)]) }
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


                                rsx!{
                                    ActionDropdown { disabled, label: "set answer constraint".to_string(), options: vec![timestamp, instance, boolean], title: "hey"  }
                                }
                            },
                        }
                    }
                }
            }
        }
    };

    let title = if is_param { "Params" } else { "Attributes" };

    rsx! {
        div {
            if disabled {
                SectionWithTitle {
                    title: title.to_string(),
                    children
                }
            } else {
                SectionWithTitle {
                    title: title.to_string(),
                    on_add: move |_| {
                        attrs.write().push(AttrEditor::new());
                    },
                    children
                }

            }
        }
    }
}

pub fn load_param_editors(card_id: CardId) -> Vec<AttrEditor> {
    let Some(card) = APP.read().try_load_card(card_id) else {
        return vec![];
    };

    if !card.is_class() {
        return vec![];
    }

    let params = card.params_on_class();

    params.into_iter().map(AttrEditor::from).collect()
}

pub fn load_attr_editors(card_id: CardId) -> Vec<AttrEditor> {
    let Some(card) = APP.read().try_load_card(card_id) else {
        return vec![];
    };

    if !card.is_class() {
        return vec![];
    }

    let attrs = card.attributes_on_class().unwrap();

    attrs.into_iter().map(AttrEditor::from).collect()
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

#[derive(PartialEq, Clone, Debug)]
pub struct ParamAnswerEditor {
    pub ty: Option<AttrBackType>,
    pub question: String,
    pub answer: Signal<Option<AttrAnswerEditor>>,
}

impl ParamAnswerEditor {
    pub fn is_same(me: Vec<Self>, them: Vec<Self>) -> bool {
        if me.len() != them.len() {
            return false;
        }

        for attr in me {
            let a_back = attr.answer.cloned().and_then(|x| x.into_backside());

            if !them
                .iter()
                .any(|b| b.answer.cloned().and_then(|x| x.into_backside()) == a_back)
            {
                return false;
            }
        }

        true
    }
}

pub fn load_param_answers_from_class(card: CardId) -> BTreeMap<AttributeId, ParamAnswerEditor> {
    let class = APP.read().try_load_card(card).unwrap();

    class
        .recursive_params_on_class()
        .into_values()
        .flatten()
        .map(|attr| {
            (
                attr.id,
                ParamAnswerEditor {
                    ty: attr.back_type,
                    question: attr.pattern.clone(),
                    answer: Signal::new_in_scope(None, ScopeId::APP),
                },
            )
        })
        .collect()
}

pub fn load_param_answers(card: CardId) -> BTreeMap<AttributeId, ParamAnswerEditor> {
    let card = APP.read().try_load_card(card).unwrap();

    let class = match card.class() {
        Some(class) => class,
        None => {
            tracing::error!("failed to retrieve class of {card}");
            return Default::default();
        }
    };

    let mut params = load_param_answers_from_class(class);

    for (id, ans) in card.param_answers() {
        let param = params.get_mut(&id).unwrap();
        let editor = AttrAnswerEditor::new(ans.answer, param.ty.clone()).unwrap();
        param.answer.clone().set(Some(editor));
    }

    params
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

    // all cards that are an attribute card based on a given instance.
    // wait, isnt this all we need? damn..
    let attr_cards_based_on_instance: BTreeSet<Arc<Card>> = card
        .attribute_cards()
        .into_iter()
        .map(|id| APP.read().load(id).unwrap())
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
        let instance = APP.read().load(instance).unwrap();
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

/// lets see... for classes, it should show all the inherited params, these cannot be edited
/// for instances, the params
pub fn load_inherited_param_editors(card_id: CardId, include_self: bool) -> Vec<AttrEditor> {
    let Some(card) = APP.read().try_load_card(card_id) else {
        return vec![];
    };

    if !card.is_class() {
        return vec![];
    }

    let mut attrs = card.params_on_parent_classes();

    if include_self {
        attrs.insert(card.id(), card.params_on_class());
    }

    let attrs: Vec<Attrv2> = attrs.into_values().flatten().collect();

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

    out
}

#[component]
fn OldAttrAnswerEditorRender(
    answer: OldAttrAnswerEditor,
    #[props(default = false)] disabled: bool,
) -> Element {
    match answer {
        OldAttrAnswerEditor::Any(answer) => {
            rsx! {
                BackPutRender {
                    text: answer.text.clone(),
                    dropdown: answer.dropdown.clone(),
                    ref_card: answer.ref_card.clone(),
                    boolean: answer.boolean.clone(),
                    disabled,
                }
            }
        }
        OldAttrAnswerEditor::Card { filter, selected } => {
            rsx! {
                ForcedCardRefRender {
                    selected_card: selected,
                    allowed: vec![CardTy::Instance],
                    filter,
                    disabled,
                }
            }
        }
        OldAttrAnswerEditor::TimeStamp(text) => {
            rsx! {
                ForcedTimestampRender { text, disabled }
            }
        }
        OldAttrAnswerEditor::Boolean(boolean) => rsx! {bool_editor { boolean, disabled }},
    }
}

#[component]
fn AttrAnswerEditorRender(
    answer: AttrAnswerEditor,
    #[props(default = false)] disabled: bool,
) -> Element {
    match answer {
        AttrAnswerEditor::TimeStamp(text) => {
            rsx! {
                TimestampRender { text, disabled }
            }
        }
        AttrAnswerEditor::Any(answer) => {
            rsx! {
                BackPutRender {
                    text: answer.text.clone(),
                    dropdown: answer.dropdown.clone(),
                    ref_card: answer.ref_card.clone(),
                    boolean: answer.boolean.clone(),
                    disabled,
                }
            }
        }
        AttrAnswerEditor::Boolean(boolean) => rsx! {opt_bool_editor { boolean, disabled }},
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
                    disabled,
                }
            }
        }
    }
}

#[component]
pub fn ParamAnswers(
    card: Option<CardId>,
    answers: Signal<BTreeMap<AttributeId, ParamAnswerEditor>>,
    class: Signal<Option<CardId>>,
    #[props(default = false)] disabled: bool,
) -> Element {
    rsx! {
        h4 {
            class: "font-bold",
            p { "Parameters" }
        }

        for (_id, ParamAnswerEditor{ ty, question, mut answer }) in answers.cloned() {
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
                                disabled,
                                onclick: move |_| {
                                    answer.set(None);
                                },
                                "X"
                            }
                            AttrAnswerEditorRender { answer: the_answer, disabled }
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
                                disabled,
                                onclick: move |_| {
                                    match ty {
                                        Some(AttrBackType::TimeStamp) => {
                                            answer.set(Some(AttrAnswerEditor::TimeStamp(Signal::new_in_scope(None, ScopeId::APP))));
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

#[component]
pub fn AttrAnswers(
    card: Option<CardId>,
    attr_answers: Signal<Vec<AttrQandA>>,
    class: Signal<Option<CardId>>,
    #[props(default = false)] disabled: bool,
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

                                OldAttrAnswerEditorRender { answer, disabled }

                                div {
                                    class: "max-h-32",
                                    DeleteButton {
                                        card_id: id,
                                        show_deps: true,
                                        disabled,
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
                                            disabled,
                                            onclick: move |_| {
                                                answer.set(None);
                                            },
                                            "X"
                                        }
                                        AttrAnswerEditorRender { answer: the_answer, disabled }
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
                                            disabled,
                                            onclick: move |_| {
                                                match attr_id.back_type {
                                                    Some(AttrBackType::TimeStamp) => {
                                                        answer.set(Some(AttrAnswerEditor::TimeStamp(Signal::new_in_scope(None, ScopeId::APP))));
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

impl OldAttrAnswerEditor {
    pub fn into_backside(self) -> Option<BackSide> {
        match self {
            OldAttrAnswerEditor::Any(back) => back.to_backside(),
            OldAttrAnswerEditor::Card {
                filter: _,
                selected,
            } => Some(BackSide::Card(selected.cloned())),
            OldAttrAnswerEditor::TimeStamp(ts) => Some(BackSide::Time(ts.cloned().parse().ok()?)),
            OldAttrAnswerEditor::Boolean(b) => Some(BackSide::Bool(b.cloned())),
        }
    }
}

impl AttrAnswerEditor {
    pub fn new(back: BackSide, ty: Option<AttrBackType>) -> Option<Self> {
        match (back, ty) {
            (BackSide::Bool(b), Some(AttrBackType::Boolean)) => {
                Some(Self::Boolean(Signal::new_in_scope(Some(b), ScopeId::APP)))
            }
            (BackSide::Time(ts), Some(AttrBackType::TimeStamp)) => Some(Self::TimeStamp(
                Signal::new_in_scope(Some(ts.serialize()), ScopeId::APP),
            )),

            (BackSide::Card(card), Some(AttrBackType::InstanceOfClass(class))) => {
                let filter = SetExpr::union_with([DynCard::Instances(class)]);
                Some(Self::Card {
                    filter,
                    instance_of: Some(class),
                    selected: Signal::new_in_scope(Some(card), ScopeId::APP),
                })
            }
            (back, None) => Some(Self::Any(BackPut::new(Some(back)))),
            (_, Some(_)) => None,
        }
    }

    pub fn into_backside(self) -> Option<BackSide> {
        match self {
            Self::Any(back) => back.to_backside(),
            Self::TimeStamp(ts) => Some(BackSide::Time(ts.cloned()?.parse().ok()?)),
            Self::Boolean(b) => Some(BackSide::Bool(b.cloned()?)),
            Self::Card { selected, .. } => Some(BackSide::Card(selected.cloned()?)),
        }
    }

    pub fn into_param_answer(self) -> Option<ParamAnswer> {
        Some(ParamAnswer {
            answer: self.into_backside()?,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AttrAnswerEditor {
    Any(BackPut),
    Card {
        filter: SetExpr,
        instance_of: Option<CardId>,
        selected: Signal<Option<CardId>>,
    },
    TimeStamp(Signal<Option<String>>),
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
