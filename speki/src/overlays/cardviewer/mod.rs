mod attributes;
mod metadata;

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    mem,
    str::FromStr,
    sync::Arc,
};

use attributes::{load_param_answers_from_class, ParamAnswerEditor};
use dioxus::prelude::*;
use ledgerstore::TheLedgerEvent;
use omtrent::TimeStamp;
use speki_core::{
    card::{AttrBackType, AttributeId, Attrv2, BackSide, CardId, ParamAnswer, TextData},
    ledger::{CardAction, CardEvent, MetaAction, MetaEvent},
    Card, CardType,
};
use tracing::info;

use crate::{
    components::{
        backside::{BackPutRender, BacksideError},
        card_mastery::MasterySection,
        cardref::{CardRefRender, OtherCardRefRender},
        frontside::FrontPutRender,
        BackPut, CardTy, DeleteButton, DropDownMenu, FrontPut, RenderDependents, SectionWithTitle,
    },
    overlays::{
        card_selector::{CardSelector, MyClosure},
        cardviewer::{
            attributes::{
                load_attr_editors, load_attr_qa, load_attr_qa_for_class,
                load_inherited_attr_editors, load_param_answers, load_param_editors,
                AttrAnswerEditor, AttrAnswers, AttrBackTypeEditor, AttrEditor, AttrQandA,
                OldAttrAnswerEditor, ParamAnswers, RenderAttrs,
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

/// container for all the structs you edit while creating/modifying a card
#[derive(Props, Clone, Debug)]
pub struct CardEditor {
    pub front: FrontPut,
    namespace: Signal<Option<CardId>>,
    back: BackPut,
    concept: Signal<Option<CardId>>,
    dependencies: Signal<Vec<CardId>>,
    allowed_cards: Vec<CardTy>,
    attrs: Signal<Vec<AttrEditor>>,
    params: Signal<Vec<AttrEditor>>,
    param_answers: Signal<BTreeMap<AttributeId, ParamAnswerEditor>>,
    inherited_attrs: Signal<Vec<AttrEditor>>,
    attr_answers: Signal<Vec<AttrQandA>>,
    fixed_concept: bool,
    metadata: MetadataEditor,
}

impl CardEditor {
    fn has_data(&self) -> bool {
        let Self {
            front,
            namespace,
            back,
            concept,
            dependencies,
            allowed_cards: _,
            attrs,
            inherited_attrs,
            attr_answers,
            fixed_concept: _,
            metadata,
            param_answers,
            params,
        } = self;

        if !params.is_empty() {
            return true;
        }

        if !param_answers.read().is_empty() {
            return true;
        }

        if !front.is_empty() {
            return true;
        }

        if !back.text.read().is_empty() || back.ref_card.card.read().is_some() {
            return true;
        }

        if namespace.read().is_some() {
            return true;
        }

        if concept.read().is_some() {
            return true;
        }

        if !dependencies.read().is_empty() {
            return true;
        }

        if !attrs.read().is_empty() {
            return true;
        }

        if !inherited_attrs.read().is_empty() {
            return true;
        }

        if !attr_answers.read().is_empty() {
            return true;
        }

        let MetadataEditor {
            trivial,
            suspended,
            needs_work,
        } = metadata;

        if needs_work.cloned() {
            return true;
        }

        if trivial.cloned() {
            return true;
        }

        if suspended.cloned() {
            return true;
        }

        false
    }

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
                            if let Some(ts) = ts.cloned() {
                                if let Err(_) = TimeStamp::from_str(&ts) {
                                    return Err("invalid timestamp".to_string());
                                }
                            }
                        }
                    }
                }
                _ => continue,
            }
        }

        let params: BTreeMap<AttributeId, Attrv2> = self
            .params
            .cloned()
            .into_iter()
            .map(|attr| {
                (
                    attr.id,
                    Attrv2 {
                        id: attr.id,
                        pattern: attr.pattern.cloned(),
                        back_type: attr.ty.cloned().map(AttrBackType::from),
                    },
                )
            })
            .collect();

        let attrs: HashMap<AttributeId, (String, Option<AttrBackType>)> = self
            .attrs
            .cloned()
            .into_iter()
            .filter_map(|AttrEditor { id, pattern, ty }| {
                let pattern = pattern.cloned();
                let ty = match ty.cloned() {
                    Some(AttrBackTypeEditor::Timestamp) => Some(AttrBackType::TimeStamp),
                    Some(AttrBackTypeEditor::Boolean) => Some(AttrBackType::Boolean),
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
                    Err(BacksideError::MissingBool) => {
                        return Err("no boolean selected".to_string());
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
                    Err(BacksideError::MissingBool) => None,
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
                    default_question: None,
                    params,
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
                    Err(BacksideError::MissingBool) => None,
                    Err(BacksideError::InvalidTimestamp) => {
                        return Err("invalid timestamp".to_string())
                    }
                };

                let answered_params: BTreeMap<AttributeId, ParamAnswer> = self
                    .param_answers
                    .cloned()
                    .into_iter()
                    .filter_map(|(id, ans)| {
                        if let Some(ans) =
                            ans.answer.cloned().and_then(|ans| ans.into_param_answer())
                        {
                            Some((id, ans))
                        } else {
                            None
                        }
                    })
                    .collect();

                CardType::Instance {
                    name: TextData::from_raw(&front),
                    back,
                    class,
                    answered_params,
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

    pub fn new_from_card(mut card: Arc<Card>) -> Result<Self, String> {
        if card.is_attribute() {
            let instance = card.attribute_instance();
            card = match APP.read().try_load_card(instance) {
                Some(card) => card,
                None => return Err("instance of attribute is missing".to_string()),
            }
        }

        let card_id = card.id();

        let raw_ty = card.clone_base();

        let front = {
            let frnt = FrontPut::new(CardTy::from_ctype(card.card_type()));
            frnt.text.clone().set(raw_ty.data.raw_front());
            frnt
        };

        let back = BackPut::new(raw_ty.data.backside().cloned());

        let editor = {
            let concept = Signal::new_in_scope(raw_ty.data.class(), ScopeId::APP);
            let param_answers: BTreeMap<AttributeId, ParamAnswerEditor> = {
                if !card.is_instance() {
                    Default::default()
                } else {
                    load_param_answers(card.id())
                }
            };
            let params = Signal::new_in_scope(
                card.params_on_class()
                    .into_iter()
                    .map(AttrEditor::from)
                    .collect(),
                ScopeId::APP,
            );

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

            let metadata = MetadataEditor::from(
                APP.read()
                    .inner()
                    .provider
                    .metadata
                    .load(card.id())
                    .unwrap_or_default(),
            );

            CardEditor {
                front,
                attrs: Signal::new_in_scope(attrs, ScopeId::APP),
                inherited_attrs: Signal::new_in_scope(inherited_attrs, ScopeId::APP),
                param_answers: Signal::new_in_scope(param_answers, ScopeId::APP),
                params,
                attr_answers,
                namespace,
                back,
                concept,
                dependencies,
                allowed_cards: vec![],
                fixed_concept: false,
                metadata,
            }
        };

        Ok(Self {
            editor,
            old_card: Some(card.clone()),
            save_hook: None,
        })
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
                attrs: Signal::new_in_scope(Default::default(), ScopeId::APP),
                params: Signal::new_in_scope(Default::default(), ScopeId::APP),
                param_answers: Signal::new_in_scope(Default::default(), ScopeId::APP),
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

    fn full_reset(&self) {
        if self.old_card.is_some() {
            debug_assert!(true);
            pop_overlay();
            return;
        }

        let CardEditor {
            front,
            namespace,
            back,
            concept,
            dependencies,
            allowed_cards: _,
            attrs,
            inherited_attrs,
            attr_answers,
            fixed_concept: _,
            metadata,
            param_answers,
            params,
        } = &self.editor;

        front.reset();
        namespace.clone().set(None);
        back.reset();
        concept.clone().set(None);
        dependencies.clone().clear();
        attrs.clone().clear();
        params.clone().clear();
        param_answers.clone().write().clear();
        inherited_attrs.clone().clear();
        attr_answers.clone().clear();
        metadata.clear();
    }

    fn light_reset(&self) {
        self.editor.front.reset();
        self.editor.back.reset();
        self.editor.dependencies.clone().write().clear();
        self.editor.attrs.clone().write().clear();
        self.editor.attr_answers.clone().write().clear();
    }
}

/*

aight..
*/

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
    inherited_attrs: Signal<Vec<AttrEditor>>,
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
            param_answers.clone().set(Default::default());
            inherited_attrs.clone().set(vec![]);
            attrs.clone().set(vec![]);
            params.clone().set(vec![]);
        }
    });

    let has_attr_answers = !attr_answers.read().is_empty();
    let has_param_answers = !param_answers.read().is_empty();
    let has_inherited_attrs = !inherited_attrs.read().is_empty();

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

                RenderAttrs { attrs: params, inherited: false, card: card_id, is_param: true }

                RenderAttrs { attrs, inherited: false, card: card_id }
                if has_inherited_attrs {
                    RenderAttrs { attrs: inherited_attrs, inherited: true, card: card_id }
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
                let mut actions: Vec<CardAction> = vec![];

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


                    let event = MetaEvent::new_modify(id, MetaAction::SetNeedsWork(meta.needs_work.cloned()));

                    if let Err(e) = APP.read().inner().provider.metadata.modify(event) {
                        let err = format!("{e:?}");
                        OverlayEnum::new_notice(err).append();
                        return;
                    }
                }


                let old_ty = selv.old_card.clone().map(|c|c.clone_base().data);

                let same_type = match &old_ty {
                    Some(old_ty) => mem::discriminant(old_ty) == mem::discriminant(&ty),
                    None => false,
                };

                match (ty, same_type) {
                    (CardType::Instance { name, back, class, answered_params }, false) => {
                        actions.push(CardAction::InstanceType { front: name, class });
                        actions.push(CardAction::SetBackside(back));
                        actions.push(CardAction::InsertParamAnswers(answered_params));

                    }
                    (CardType::Instance { name, back, class, answered_params }, true) => {
                        actions.push(CardAction::SetFront(name));
                        actions.push(CardAction::SetBackside(back));
                        actions.push(CardAction::SetInstanceClass(class));
                        actions.push(CardAction::InsertParamAnswers(answered_params));
                    },
                    (CardType::Normal { front, back }, true) => {
                        actions.push(CardAction::SetFront(front));
                        actions.push(CardAction::SetBackside(Some(back)));

                    },
                    (CardType::Normal { front, back }, false) => {
                        actions.push(CardAction::NormalType {front, back});
                    },
                    (CardType::Unfinished { front }, true) => {
                        actions.push(CardAction::SetFront(front));
                    },
                    (CardType::Unfinished { front }, false) => {
                        actions.push(CardAction::UnfinishedType {front});
                    },
                    (CardType::Attribute { attribute, back, instance }, true) => {
                        actions.push(CardAction::AttributeType {attribute, back, instance});
                    }
                    (CardType::Attribute { attribute, back, instance }, false) => {
                        actions.push(CardAction::AttributeType {attribute, back, instance});
                    },
                    (CardType::Class { name, back, parent_class, default_question: _, attrs, params }, true) => {
                        actions.push(CardAction::SetFront(name));
                        actions.push(CardAction::SetBackside(back));
                        actions.push(CardAction::SetParentClass(parent_class));
                        actions.push(CardAction::InsertAttrs(attrs));
                        actions.push(CardAction::InsertParams(params.into_values().collect()));
                    },
                    (CardType::Class { name, back, parent_class, default_question: _, attrs, params }, false) => {
                        actions.push(CardAction::ClassType { front: name });
                        actions.push(CardAction::SetBackside(back));
                        actions.push(CardAction::SetParentClass(parent_class));
                        actions.push(CardAction::InsertAttrs(attrs));
                        actions.push(CardAction::InsertParams(params.into_values().collect()));
                    },
                    (CardType::Statement { front }, true) => {
                        actions.push(CardAction::SetFront(front));
                    },
                    (CardType::Statement { front }, false) => {
                        actions.push(CardAction::StatementType { front });
                    },
                    (CardType::Event {..}, _) => {
                        todo!()
                    },
                }

                actions.push(CardAction::SetNamespace ( namespace));
                actions.push(CardAction::SetTrivial ( meta.trivial.cloned()));

                for dep in deps {
                    actions.push(CardAction::AddDependency(dep));
                }

                for event in actions {
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
                                    AttrAnswerEditor::Boolean(boolean) => {
                                        if let Some(boolean) = boolean.cloned() {
                                            let back = BackSide::Bool(boolean);
                                            let action = CardAction::AttributeType { attribute: attr_id.id, back , instance: id };
                                            let event = CardEvent::new_modify(CardId::new_v4(), action);
                                            if let Err(e) = APP.read().inner().provider.cards.modify(event) {
                                                handle_card_event_error(e);
                                                return;
                                            }

                                        }
                                    },
                                    AttrAnswerEditor::TimeStamp(ts) => {
                                        if let Some(ts) = ts.cloned() {
                                            if let Ok(ts) = TimeStamp::from_str(&ts) {
                                                let back = BackSide::Time(ts.clone());
                                                let action = CardAction::AttributeType { attribute: attr_id.id, back , instance: id };
                                                let event = CardEvent::new_modify(CardId::new_v4(), action);
                                                if let Err(e) = APP.read().inner().provider.cards.modify(event) {
                                                    handle_card_event_error(e);
                                                    return;
                                                }
                                            }
                                        }
                                    },
                                    AttrAnswerEditor::Any(back_put) => {
                                        if let Some(back) = back_put.to_backside() {
                                            let action = CardAction::AttributeType { attribute: attr_id.id, back , instance: id };
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
                                            let action = CardAction::AttributeType { attribute: attr_id.id, back , instance: id };
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
                                OldAttrAnswerEditor::Boolean(boolean) => {
                                    let boolean = boolean.cloned();
                                    let prev_back = APP.read().inner().card_provider.providers.cards.load(attr_card_id).unwrap().ref_backside().cloned().unwrap();
                                    let back = BackSide::Bool(boolean);
                                        if prev_back != back {
                                            let action = CardAction::SetBackBool(boolean);
                                            let event = CardEvent::new_modify(attr_card_id, action);
                                            if let Err(e) = APP.read().inner().provider.cards.modify(event) {
                                                handle_card_event_error(e);
                                                return;
                                            }
                                        }

                                }
                                OldAttrAnswerEditor::TimeStamp(ts) => {
                                    let prev_back = APP.read().inner().card_provider.providers.cards.load(attr_card_id).unwrap().ref_backside().cloned().unwrap();
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
                                    let prev_back = APP.read().inner().card_provider.providers.cards.load(attr_card_id).unwrap().ref_backside().cloned().unwrap();
                                    if let Some(back) = back_put.to_backside() {
                                        if back != prev_back {
                                            let action = CardAction::AttributeType { attribute: attr_id, back , instance: id };
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
                                    let action = CardAction::AttributeType { attribute: attr_id, back , instance: id };
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
                    selveste.light_reset();
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
