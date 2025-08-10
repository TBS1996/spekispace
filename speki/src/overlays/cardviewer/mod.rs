mod attributes;
mod cardeditor;
mod metadata;

use std::{
    collections::{BTreeMap, BTreeSet},
    fs, mem,
    str::FromStr,
    sync::Arc,
};

use attributes::{load_param_answers_from_class, ParamAnswerEditor};
use dioxus::prelude::*;
use ledgerstore::LedgerEvent;
use omtrent::TimeStamp;
use rfd::FileDialog;
use speki_core::{
    card::{AttributeId, BackSide, CardId},
    ledger::{CardAction, CardEvent, MetaAction, MetaEvent},
    set::{Input, SetAction, SetEvent, SetExpr, SetId},
    Card, CardType,
};
use tracing::info;

pub use cardeditor::CardViewer;

use crate::{
    components::{
        backside::{BackOpts, BackPutRender},
        card_mastery::MasterySection,
        cardref::{CardRefRender, OtherCardRefRender},
        frontside::FrontPutRender,
        BackPut, CardTy, DeleteButton, FrontPut, RenderDependents, SectionWithTitle,
    },
    overlays::{
        card_selector::{CardSelector, MyClosure},
        cardviewer::{
            attributes::{
                load_attr_editors, load_attr_qa, load_attr_qa_for_class,
                load_inherited_attr_editors, load_param_answers, load_param_editors,
                AttrAnswerEditor, AttrAnswers, AttrEditor, AttrQandA, OldAttrAnswerEditor,
                ParamAnswers, RenderAttrs, RenderInheritedAttrs,
            },
            metadata::{DisplayMetadata, MetadataEditor},
        },
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
            concept: viewer.editor.concept.clone(),
            ty: ty.cloned(),
            card_id,
            namespace: viewer.editor.namespace.clone(),
            attrs: viewer.editor.attrs.clone(),
            params: viewer.editor.params.clone(),
            inherited_attrs: viewer.editor.inherited_attrs.clone(),
            inherited_params: viewer.editor.inherited_params.clone(),
            attr_answers: viewer.editor.attr_answers.clone(),
            fixed_concept: viewer.editor.fixed_concept,
            param_answers: viewer.editor.param_answers.clone(),
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

    let props_for_clear = props.clone();
    let clear_enabled = props.old_card.is_none() && props.editor.has_data();

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
                        } else {
                            button {
                                class: "{crate::styles::GRAY_BUTTON}",
                                disabled: !clear_enabled,
                                onclick: move |_| {
                                    props_for_clear.full_reset();
                                },
                                "Clear"
                            }

                        }
                    }

                    div { class: "flex-grow" }

                    if props.show_import {
                        ImportCards {viewer: props.clone()}
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

    let children = rsx! {
        for (idx, (name, id)) in name_and_id.into_iter().enumerate() {
            div {
                class: "flex flex-row",
                button {
                    class: "mb-1 p-1 bg-gray-100 rounded-md text-left",
                    onclick: move |_| {
                        if let Some(card) = APP.read().try_load_card(id) {
                            match CardViewer::new_from_card(card) {
                                Ok(viewer) => OverlayEnum::CardViewer(viewer).append(),
                                Err(s) => OverlayEnum::new_notice(s).append(),
                            }
                        }
                    },
                    "{name}"
                }

                button {
                    class: "p-1 hover:bg-gray-200 hover:border-gray-400 border border-transparent rounded-md transition-colors",
                    onclick: move |_| {
                        let removed = dependencies.write().remove(idx);
                        if let Some(id) = card_id {
                            let event = LedgerEvent::new_modify(id, CardAction::RemoveDependency(removed));
                            if let Err(e) = APP.read().inner().provider.cards.modify(event) {
                                handle_card_event_error(e);
                            }
                        }
                    },
                    "X"
                }
            }
        }
    };

    rsx! {
        div {
            class: "flex flex-col opacity-100 visible w-full h-auto bg-white p-2 shadow-md rounded-md overflow-y-auto",
            SectionWithTitle {
                title: "Explicit dependencies".to_string(),
                on_add: move |_| {
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
                },
                children
            }
        }
    }
}

enum _TheEditor {
    /// so let's persist the param answers
    Instance {
        param_answers: ReadOnlySignal<BTreeMap<AttributeId, ParamAnswerEditor>>,
    },
    Class {
        params: Signal<Vec<AttrEditor>>,
        attrs: Signal<Vec<AttrEditor>>,
        inherited_attrs: ReadOnlySignal<Vec<AttrEditor>>,
        inherited_params: ReadOnlySignal<Vec<AttrEditor>>,
    },
}

#[component]
pub fn ImportCards(viewer: CardViewer) -> Element {
    let editor = viewer.editor.clone();
    rsx! {
        div {
            button {
                class: "{crate::styles::CREATE_BUTTON}",
                onclick: move |_| {
                    if let Some(path) = FileDialog::new().pick_file() {
                        let back_requirement = match editor.front.dropdown.selected.cloned() {
                            CardTy::Normal => Some(true),
                            CardTy::Instance => None,
                            CardTy::Class => None,
                            CardTy::Unfinished => Some(false),
                        };

                        let guide = match back_requirement {
                            Some(true) => "Each row must contain two columns, front and back, tab separated",
                            Some(false) => "Each row must contain only the frontside",
                            None => "Each row must have a frontside and an optional backside, tab separated",
                        };

                        let filename = path.file_stem().unwrap().to_str().unwrap().to_string();

                        let s = match fs::read_to_string(path) {
                            Ok(s) => s,
                            Err(_) => {
                                OverlayEnum::new_notice("file must be valid tsv".to_string()).append();
                                return;
                            }
                        };

                        let mut front_and_back: Vec<(String, Option<String>)> = vec![];

                        for line in s.lines() {
                            let splits: Vec<&str> = line.split('\t').collect();
                            let split_qty = splits.len();

                            let is_valid = match back_requirement {
                                Some(true) => split_qty == 2,
                                Some(false) => split_qty == 1,
                                None => split_qty == 1 || split_qty == 2,
                            };

                            if !is_valid {
                                let err_msg = format!("Error: {guide}\ninvalid row: {line}");
                                OverlayEnum::new_notice(err_msg).append();
                                return;
                            }

                            let front = splits[0].to_owned();
                            let back = splits.get(1).map(ToOwned::to_owned).map(ToOwned::to_owned);
                            front_and_back.push((front, back));
                        }

                        let mut reps: Vec<CardRep> = vec![];

                        for (front, back) in front_and_back {
                            let mut editor = editor.clone();
                            editor.front.text.set(front);
                            editor.back.dropdown.set(BackOpts::Text);
                            editor.back.text.set(back.unwrap_or_default());
                            match editor.into_cardrep() {
                                Ok(rep) => reps.push(rep),
                                Err(e) => {
                                    let err = format!("failed to create card: {e}");
                                    OverlayEnum::new_notice(err).append();
                                    return;
                                },
                            };
                        }

                        let set_id = SetId::new_v4();
                        let event = SetEvent::new_modify(set_id, SetAction::SetName(filename));
                        APP.read().inner().provider.sets.modify(event).unwrap();
                        let mut saved_cards: BTreeSet<Input> = Default::default();

                        for rep in reps {
                            if let Ok(card) = save_cardrep(rep, None) {
                                saved_cards.insert(Input::Card(card));
                            }
                        }

                        let cards_imported =saved_cards.len();

                        let expr = SetExpr::Union(saved_cards);
                        let event = SetEvent::new_modify(set_id, SetAction::SetExpr(expr));
                        APP.read().inner().provider.sets.modify(event).unwrap();
                        viewer.light_reset();
                        let notice = format!("imported {cards_imported} cards");
                        OverlayEnum::new_notice(notice).append();

                    }
                },
                "Import"
            }
        }
    }
}

#[component]
fn InputElements(
    front: FrontPut,
    back: BackPut,
    concept: Signal<Option<CardId>>,
    ty: CardTy,
    card_id: Option<CardId>,
    namespace: Signal<Option<CardId>>,
    attrs: Signal<Vec<AttrEditor>>,
    params: Signal<Vec<AttrEditor>>,
    inherited_params: Memo<Vec<AttrEditor>>,
    inherited_attrs: Memo<Vec<AttrEditor>>,
    attr_answers: Signal<Vec<AttrQandA>>,
    param_answers: Signal<BTreeMap<AttributeId, ParamAnswerEditor>>,
    fixed_concept: bool,
) -> Element {
    use_effect(move || match ((front.dropdown.selected)(), concept()) {
        (CardTy::Instance, Some(class)) => {
            dbg!("#######################");
            let _ = attr_answers.clone();
            let mut new_params: BTreeMap<AttributeId, ParamAnswerEditor> = match card_id {
                Some(card) => load_param_answers(card),
                None => load_param_answers_from_class(class),
            };

            let old_params = param_answers.cloned();

            let mut new_keys: Vec<&AttributeId> = new_params.keys().collect();
            new_keys.sort();
            let mut old_keys: Vec<&AttributeId> = old_params.keys().collect();
            old_keys.sort();

            dbg!(&new_keys, &old_keys);

            if new_keys != old_keys {
                dbg!(&old_params, &new_params);

                for (id, ans) in &old_params {
                    if new_params.get(id).is_some() {
                        new_params.insert(*id, ans.to_owned());
                    }
                }

                param_answers.set(new_params);
                dbg!(&param_answers);
            }
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
                    {
                        let mut old = params.cloned();
                        let new_attrs = load_param_editors(card);

                        let new_attr_ids: BTreeSet<AttributeId> =
                            new_attrs.iter().map(|a| a.id).collect();

                        // Retain old ones if their ID is in the new set
                        old.retain(|a| new_attr_ids.contains(&a.id));

                        // Add new ones if their ID wasn't already in old
                        let old_attr_ids: BTreeSet<AttributeId> =
                            old.iter().map(|a| a.id).collect();

                        let mut combined = old;
                        combined.extend(
                            new_attrs
                                .into_iter()
                                .filter(|a| !old_attr_ids.contains(&a.id)),
                        );
                        if old_attr_ids != new_attr_ids {
                            params.clone().set(combined);
                        }
                    }

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
                }
                None => match class {
                    Some(class) => {
                        let mut new_attrs = load_inherited_attr_editors(class);
                        new_attrs.extend(load_attr_editors(class));
                    }
                    None => {}
                },
            }
        }
        (_, _) => {
            attr_answers.clone().set(vec![]);
            param_answers.clone().set(Default::default());
            attrs.clone().set(vec![]);
            params.clone().set(vec![]);
        }
    });

    let has_attr_answers = !attr_answers.read().is_empty();
    let has_param_answers = !param_answers.read().is_empty();
    let has_inherited_attrs = !inherited_attrs.read().is_empty();
    let has_inherited_params = !inherited_params.read().is_empty();

    dbg!(&attr_answers);

    rsx! {
        FrontPutRender { dropdown: front.dropdown.clone(), text: front.text.clone()}

        match ty {
            CardTy::Unfinished => rsx! {},

            CardTy::Normal => rsx! {
                BackPutRender {
                    text: back.text.clone(),
                    dropdown: back.dropdown.clone(),
                    ref_card: back.ref_card.clone(),
                    boolean: back.boolean.clone(),
                }
            },
            CardTy::Class => rsx! {
                BackPutRender {
                    text: back.text.clone(),
                    dropdown: back.dropdown.clone(),
                    ref_card: back.ref_card.clone(),
                    boolean: back.boolean.clone(),
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

                RenderAttrs { attrs: params, card: card_id, is_param: true }

                if has_inherited_params {
                    RenderInheritedAttrs { attrs: inherited_params, card: card_id, is_param: true }
                }

                RenderAttrs { attrs, card: card_id }
                if has_inherited_attrs {
                    RenderInheritedAttrs { attrs: inherited_attrs, card: card_id }
                }
            },
            CardTy::Instance => rsx! {
                BackPutRender {
                    text: back.text.clone(),
                    dropdown: back.dropdown.clone(),
                    ref_card: back.ref_card.clone(),
                    boolean: back.boolean.clone(),
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

                if has_param_answers {
                    ParamAnswers { card: card_id, answers: param_answers, class: concept  }
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

fn save_cardrep(rep: CardRep, old_card: Option<Arc<Card>>) -> Result<CardId, ()> {
    let mut actions: Vec<CardAction> = vec![];

    let CardRep {
        ty,
        namespace,
        deps,
        answered_attrs,
        meta,
    } = rep;

    let id = old_card
        .clone()
        .map(|card| card.id())
        .unwrap_or_else(CardId::new_v4);

    {
        let event = MetaEvent::new_modify(id, MetaAction::Suspend(meta.suspended.cloned()));

        if let Err(e) = APP.read().inner().provider.metadata.modify(event) {
            let err = format!("{e:?}");
            OverlayEnum::new_notice(err).append();
            return Err(());
        }

        let event = MetaEvent::new_modify(id, MetaAction::SetTrivial(Some(meta.trivial.cloned())));

        if let Err(e) = APP.read().inner().provider.metadata.modify(event) {
            let err = format!("{e:?}");
            OverlayEnum::new_notice(err).append();
            return Err(());
        }

        let event = MetaEvent::new_modify(id, MetaAction::SetNeedsWork(meta.needs_work.cloned()));

        if let Err(e) = APP.read().inner().provider.metadata.modify(event) {
            let err = format!("{e:?}");
            OverlayEnum::new_notice(err).append();
            return Err(());
        }
    }

    let old_ty = old_card.clone().map(|c| c.clone_base().data);

    let same_type = match &old_ty {
        Some(old_ty) => mem::discriminant(old_ty) == mem::discriminant(&ty),
        None => false,
    };

    match (ty, same_type) {
        (
            CardType::Instance {
                name,
                back,
                class,
                answered_params,
            },
            false,
        ) => {
            actions.push(CardAction::InstanceType { front: name, class });
            actions.push(CardAction::SetBackside(back));
            actions.push(CardAction::InsertParamAnswers(answered_params));
        }
        (
            CardType::Instance {
                name,
                back,
                class,
                answered_params,
            },
            true,
        ) => {
            actions.push(CardAction::SetFront(name));
            actions.push(CardAction::SetBackside(back));
            actions.push(CardAction::SetInstanceClass(class));
            actions.push(CardAction::InsertParamAnswers(answered_params));
        }
        (CardType::Normal { front, back }, true) => {
            actions.push(CardAction::SetFront(front));
            actions.push(CardAction::SetBackside(Some(back)));
        }
        (CardType::Normal { front, back }, false) => {
            actions.push(CardAction::NormalType { front, back });
        }
        (CardType::Unfinished { front }, true) => {
            actions.push(CardAction::SetFront(front));
        }
        (CardType::Unfinished { front }, false) => {
            actions.push(CardAction::UnfinishedType { front });
        }
        (
            CardType::Attribute {
                attribute,
                back,
                instance,
            },
            true,
        ) => {
            actions.push(CardAction::AttributeType {
                attribute,
                back,
                instance,
            });
        }
        (
            CardType::Attribute {
                attribute,
                back,
                instance,
            },
            false,
        ) => {
            actions.push(CardAction::AttributeType {
                attribute,
                back,
                instance,
            });
        }
        (
            CardType::Class {
                name,
                back,
                parent_class,
                default_question: _,
                attrs,
                params,
            },
            true,
        ) => {
            actions.push(CardAction::SetFront(name));
            actions.push(CardAction::SetBackside(back));
            actions.push(CardAction::SetParentClass(parent_class));
            actions.push(CardAction::InsertAttrs(attrs));
            actions.push(CardAction::InsertParams(params.into_values().collect()));
        }
        (
            CardType::Class {
                name,
                back,
                parent_class,
                default_question: _,
                attrs,
                params,
            },
            false,
        ) => {
            actions.push(CardAction::ClassType { front: name });
            actions.push(CardAction::SetBackside(back));
            actions.push(CardAction::SetParentClass(parent_class));
            actions.push(CardAction::InsertAttrs(attrs));
            actions.push(CardAction::InsertParams(params.into_values().collect()));
        }
        (CardType::Statement { front }, true) => {
            actions.push(CardAction::SetFront(front));
        }
        (CardType::Statement { front }, false) => {
            actions.push(CardAction::StatementType { front });
        }
        (CardType::Event { .. }, _) => {
            todo!()
        }
    }

    actions.push(CardAction::SetNamespace(namespace));
    actions.push(CardAction::SetTrivial(meta.trivial.cloned()));

    for dep in deps {
        actions.push(CardAction::AddDependency(dep));
    }

    for event in actions {
        let event = CardEvent::new_modify(id, event);
        if let Err(e) = APP.read().inner().provider.cards.modify(event) {
            handle_card_event_error(e);
            return Err(());
        }
    }

    for answer in answered_attrs {
        match answer {
            AttrQandA::New {
                attr_id,
                question: _,
                answer,
            } => {
                if let Some(answer) = answer.cloned() {
                    match answer {
                        AttrAnswerEditor::Boolean(boolean) => {
                            if let Some(boolean) = boolean.cloned() {
                                let back = BackSide::Bool(boolean);
                                let action = CardAction::AttributeType {
                                    attribute: attr_id.id,
                                    back,
                                    instance: id,
                                };
                                let event = CardEvent::new_modify(CardId::new_v4(), action);
                                if let Err(e) = APP.read().inner().provider.cards.modify(event) {
                                    handle_card_event_error(e);
                                    return Err(());
                                }
                            }
                        }
                        AttrAnswerEditor::TimeStamp(ts) => {
                            if let Some(ts) = ts.cloned() {
                                if let Ok(ts) = TimeStamp::from_str(&ts) {
                                    let back = BackSide::Time(ts.clone());
                                    let action = CardAction::AttributeType {
                                        attribute: attr_id.id,
                                        back,
                                        instance: id,
                                    };
                                    let event = CardEvent::new_modify(CardId::new_v4(), action);
                                    if let Err(e) = APP.read().inner().provider.cards.modify(event)
                                    {
                                        handle_card_event_error(e);
                                        return Err(());
                                    }
                                }
                            }
                        }
                        AttrAnswerEditor::Any(back_put) => {
                            if let Some(back) = back_put.to_backside() {
                                let action = CardAction::AttributeType {
                                    attribute: attr_id.id,
                                    back,
                                    instance: id,
                                };
                                let event = CardEvent::new_modify(CardId::new_v4(), action);
                                if let Err(e) = APP.read().inner().provider.cards.modify(event) {
                                    handle_card_event_error(e);
                                    return Err(());
                                }
                            }
                        }
                        AttrAnswerEditor::Card {
                            filter: _,
                            instance_of: _,
                            selected,
                        } => {
                            if let Some(card) = selected.cloned() {
                                let back = BackSide::Card(card);
                                let action = CardAction::AttributeType {
                                    attribute: attr_id.id,
                                    back,
                                    instance: id,
                                };
                                let event = CardEvent::new_modify(CardId::new_v4(), action);
                                if let Err(e) = APP.read().inner().provider.cards.modify(event) {
                                    handle_card_event_error(e);
                                    return Err(());
                                }
                            }
                        }
                    }
                }
            }
            AttrQandA::Old {
                id: attr_card_id,
                question: _,
                answer,
                attr_id,
            } => match answer {
                OldAttrAnswerEditor::Boolean(boolean) => {
                    let boolean = boolean.cloned();
                    let prev_back = APP
                        .read()
                        .inner()
                        .card_provider
                        .providers
                        .cards
                        .load(attr_card_id)
                        .unwrap()
                        .ref_backside()
                        .cloned()
                        .unwrap();
                    let back = BackSide::Bool(boolean);
                    if prev_back != back {
                        let action = CardAction::SetBackBool(boolean);
                        let event = CardEvent::new_modify(attr_card_id, action);
                        if let Err(e) = APP.read().inner().provider.cards.modify(event) {
                            handle_card_event_error(e);
                            return Err(());
                        }
                    }
                }
                OldAttrAnswerEditor::TimeStamp(ts) => {
                    let prev_back = APP
                        .read()
                        .inner()
                        .card_provider
                        .providers
                        .cards
                        .load(attr_card_id)
                        .unwrap()
                        .ref_backside()
                        .cloned()
                        .unwrap();
                    if let Ok(ts) = TimeStamp::from_str(&ts.cloned()) {
                        let back = BackSide::Time(ts.clone());
                        if prev_back != back {
                            let action = CardAction::SetBackTime(ts);
                            let event = CardEvent::new_modify(attr_card_id, action);
                            if let Err(e) = APP.read().inner().provider.cards.modify(event) {
                                handle_card_event_error(e);
                                return Err(());
                            }
                        }
                    }
                }

                OldAttrAnswerEditor::Any(back_put) => {
                    let prev_back = APP
                        .read()
                        .inner()
                        .card_provider
                        .providers
                        .cards
                        .load(attr_card_id)
                        .unwrap()
                        .ref_backside()
                        .cloned()
                        .unwrap();
                    if let Some(back) = back_put.to_backside() {
                        if back != prev_back {
                            let action = CardAction::AttributeType {
                                attribute: attr_id,
                                back,
                                instance: id,
                            };
                            let event = CardEvent::new_modify(attr_card_id, action);
                            if let Err(e) = APP.read().inner().provider.cards.modify(event) {
                                handle_card_event_error(e);
                                return Err(());
                            }
                        }
                    }
                }
                OldAttrAnswerEditor::Card {
                    filter: _,
                    selected,
                } => {
                    let card = selected.cloned();
                    let back = BackSide::Card(card);
                    let action = CardAction::AttributeType {
                        attribute: attr_id,
                        back,
                        instance: id,
                    };
                    let event = CardEvent::new_modify(attr_card_id, action);
                    if let Err(e) = APP.read().inner().provider.cards.modify(event) {
                        handle_card_event_error(e);
                        return Err(());
                    }
                }
            },
        }
    }

    Ok(id)
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
                let Ok(rep) = CardViewer.editor.clone().into_cardrep() else {
                    return;
                };

                match save_cardrep(rep, CardViewer.old_card.clone()) {
                    Ok(id) => {
                        let Some(card) = APP.read().inner().card_provider().load(id) else {
                            dbg!(id);
                            panic!();
                        };

                        let inner_card = Arc::unwrap_or_clone(card);
                        if let Some(hook) = CardViewer.save_hook.clone() {
                            hook.call(inner_card.id());
                        } else {
                            CardViewer.light_reset();
                            pop_overlay();
                        }

                    },
                    Err(()) => {},
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
