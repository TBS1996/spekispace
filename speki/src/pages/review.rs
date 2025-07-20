use crate::{
    components::{
        dropdown::{ActionDropdown, DropComponent, DropdownAction},
        FilterComp, FilterEditor, SectionWithTitle,
    },
    overlays::{
        card_selector::{CardSelector, MaybeEntry, MyClosure},
        Overender, OverlayChoice, OverlayEnum, OverlaySelector,
    },
    APP,
};
use crate::{styles, OVERLAY};
use dioxus::prelude::*;
use ledgerstore::TheLedgerEvent;
use nonempty::NonEmpty;
use speki_core::card::CType;
use speki_core::{
    card::CardId,
    cardfilter::CardFilter,
    collection::{DynCard, MaybeCard},
    set::{Input, Set, SetAction, SetEvent, SetExpr, SetExprDiscriminants, SetId},
    Card,
};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet, HashSet},
    fmt::Debug,
    sync::Arc,
};
use strum::IntoEnumIterator;
use tracing::info;
use uuid::Uuid;

#[derive(Clone)]
pub struct ReviewPage {
    filter: FilterEditor,
    sets: Signal<Vec<SetEditor>>,
    cardfilter: Memo<CardFilter>,
}

fn load_sets() -> Vec<SetEditor> {
    let mut sets: Vec<SetEditor> = APP
        .read()
        .inner()
        .provider
        .sets
        .load_all()
        .into_iter()
        .map(|set| SetEditor::new(&set))
        .collect();

    sets.sort_by_key(|set| set.name.cloned());
    sets
}

impl ReviewPage {
    pub fn new() -> Self {
        let filter = FilterEditor::new_default();
        let cardfilter = filter.memo();

        let sets: Signal<Vec<SetEditor>> = Signal::new_in_scope(load_sets(), ScopeId::APP);

        Self {
            filter,
            sets,
            cardfilter,
        }
    }
}

#[component]
pub fn Review() -> Element {
    let state: ReviewPage = use_context::<ReviewPage>();
    let editor = state.filter.clone();
    tracing::info!("memo lol: {:?}", &state.cardfilter);

    let overlay = OVERLAY.read().get();
    let sets = state.sets.clone();

    rsx! {
        Overender {
            overlay,
            root: rsx!{
                div {
                    class: "flex flex-row items-start min-h-screen gap-x-6 justify-start w-full",
                    SectionWithTitle {
                        title: "Filter".to_string(),
                        FilterComp { editor }
                    }

                    SectionWithTitle {
                        title: "Sets".to_string(),
                        RenderSets {filter: state.filter.to_filter(), sets }
                     }

                }
            }
        }
    }
}

#[component]
fn RenderInput(
    filter: CardFilter,
    input: InputEditor,
    #[props(default = 0)] depth: usize,
) -> Element {
    let ledger = APP.read().inner().provider.sets.clone();

    let leaf = match input {
        InputEditor::Card(card) => {
            let name = APP
                .read()
                .inner()
                .card_provider
                .load(card)
                .map(|card| card.name().to_string())
                .unwrap_or("<invalid card>".to_string());
            Some(name)
        }
        InputEditor::Leaf(card) => {
            let provider = APP.read().inner().card_provider.clone();
            Some(card.display(provider))
        }
        _ => None,
    }
    .unwrap_or_default();
    let class = format!("pl-{}", depth * 4);

    rsx! {
        div {
        class: "{class}",

        match input {
            InputEditor::Card(_) => {
                rsx!{
                    p { "{leaf}" }}
            },
            InputEditor::Leaf(_) => {
                rsx!{
                    p { "{leaf}" }}
            },
            InputEditor::Reference(id) => {
                rsx!{RenderSet { filter, set: SetEditor::new(&ledger.load(id)), depth: depth + 1}}
            },
            InputEditor::Expr(expr) => {
                rsx!{

                    RenderExpr {filter, inputs: expr.inputs.clone(), ty: expr.ty.clone(), depth: depth + 1}
                }
            },
        }
    }
    }
}

fn card_choice(ty: CType, inputs: Signal<Vec<InputEditor>>) -> OverlayChoice {
    let f: Arc<Box<dyn Fn() -> Option<OverlayEnum>>> = {
        let f = move || {
            let leaf = DynCard::CardType(ty);
            let input = InputEditor::Leaf(leaf);
            inputs.clone().write().push(input);
            None
        };
        Arc::new(Box::new(f))
    };

    OverlayChoice {
        display: format!("{ty} cards"),
        overlay: f,
    }
}

#[component]
pub fn RenderExpr(
    filter: CardFilter,
    inputs: Signal<Vec<InputEditor>>,
    ty: Signal<SetExprDiscriminants>,
    #[props(default = 0)] depth: usize,
) -> Element {
    let class = format!("pl-{}", depth * 4);

    let expr_func = Box::new(move || {
        let expr = SetExpr::default();
        let input: InputEditor = Input::Expr(expr.into()).into();
        inputs.clone().write().push(input);
    });

    let leaf_func = Box::new(move || {
        // normal card
        let leaf_card = {
            let f: Arc<Box<dyn Fn() -> Option<OverlayEnum>>> = {
                let f = move || {
                    let f = move |card: CardId| {
                        let input = InputEditor::Card(card);
                        inputs.clone().write().push(input);
                    };
                    let overlay = CardSelector::new(false, vec![])
                        .new_on_card_selected(MyClosure::new(f), true);
                    Some(OverlayEnum::CardSelector(overlay))
                };

                Arc::new(Box::new(f))
            };

            OverlayChoice {
                display: "single card".to_string(),
                overlay: f,
            }
        };

        let class_cards = card_choice(CType::Class, inputs.clone());
        let normal_cards = card_choice(CType::Normal, inputs.clone());
        let instance_cards = card_choice(CType::Instance, inputs.clone());
        let attr_cards = card_choice(CType::Attribute, inputs.clone());
        let unfinished_cards = card_choice(CType::Unfinished, inputs.clone());

        let trivial: OverlayChoice = {
            let f: Arc<Box<dyn Fn() -> Option<OverlayEnum>>> = {
                let f = move || {
                    let leaf = DynCard::Trivial(true);
                    let input = InputEditor::Leaf(leaf);
                    inputs.clone().write().push(input);
                    None
                };
                Arc::new(Box::new(f))
            };

            OverlayChoice {
                display: format!("trivial cards"),
                overlay: f,
            }
        };

        let instances = {
            let f: Arc<Box<dyn Fn() -> Option<OverlayEnum>>> = {
                let f = move || {
                    let f = move |card: CardId| {
                        let leaf = DynCard::Instances(card);
                        let input = InputEditor::Leaf(leaf);
                        inputs.clone().write().push(input);
                    };
                    let overlay = CardSelector::class_picker(MyClosure::new(f));
                    Some(OverlayEnum::CardSelector(overlay))
                };

                Arc::new(Box::new(f))
            };

            OverlayChoice {
                display: "instances".to_string(),
                overlay: f,
            }
        };

        let dependents = {
            let f: Arc<Box<dyn Fn() -> Option<OverlayEnum>>> = {
                let f = move || {
                    let f = move |card: CardId| {
                        let leaf = DynCard::RecDependents(card);
                        let input = InputEditor::Leaf(leaf);
                        inputs.clone().write().push(input);
                    };
                    let overlay = CardSelector::new(false, vec![])
                        .new_on_card_selected(MyClosure::new(f), true);
                    Some(OverlayEnum::CardSelector(overlay))
                };

                Arc::new(Box::new(f))
            };

            OverlayChoice {
                display: "dependents".to_string(),
                overlay: f,
            }
        };

        let sel = OverlaySelector {
            title: "leaf type".to_string(),
            choices: vec![
                leaf_card,
                instances,
                dependents,
                normal_cards,
                class_cards,
                instance_cards,
                attr_cards,
                unfinished_cards,
                trivial,
            ],
            chosen: None,
        };
        OverlayEnum::OverlaySelector(sel).append();
    });

    let expr_opt = DropdownAction::new("expr".to_string(), expr_func);
    let leaf_opt = DropdownAction::new("leaf".to_string(), leaf_func);

    let enable_new_input = match ty.cloned() {
        SetExprDiscriminants::Union => true,
        SetExprDiscriminants::Intersection => true,
        SetExprDiscriminants::Difference => inputs.read().len() < 2,
        SetExprDiscriminants::Complement => inputs.read().is_empty(),
        SetExprDiscriminants::All => false,
    };

    rsx! {
        div {
            class: "{class}",

            div {
                class: "flex flex-row",
                DropComponent { options: SetExprDiscriminants::iter().collect(), selected: ty}

                if enable_new_input {
                    ActionDropdown {label:"➕".to_string(), options: vec![expr_opt,leaf_opt]  }
                }
            }


            for input in inputs.cloned() {
                div {
                    class: "flex flex-row items-start {class}",
                    button {
                        class: "mt-1", // optional: fine-tune vertical alignment
                        onclick: move |_| {
                            inputs.write().retain(|x|x != &input);
                        },
                        "🗑️"
                    }
                    RenderInput { filter: filter.clone(), input: input.clone(), depth: depth + 1 }
                }
            }
        }
    }
}

#[component]
fn RenderSet(
    filter: CardFilter,
    set: SetEditor,
    #[props(default = 0)] depth: usize,
    #[props(default = true)] editable: bool,
) -> Element {
    let SetEditor {
        id,
        mut name,
        expr,
        to_delete: mut delete_atomic,
        show_edit,
    } = set.clone();

    let ledger = APP.read().inner().provider.sets.clone();
    let filter2 = filter.clone();

    let real_set = SetExpr::try_from(expr.cloned());

    let (save_button, is_new): (bool, bool) = match real_set.clone() {
        Ok(set_expr) => match APP.read().inner().provider.sets.try_load(set.id) {
            Some(old_set) => (
                old_set.expr != set_expr || old_set.name != set.name.cloned(),
                false,
            ),
            None => (true, false),
        },
        Err(_) => (false, true),
    };

    let save_button = save_button && editable;
    let save_class = if is_new {
        crate::styles::CREATE_BUTTON
    } else {
        crate::styles::UPDATE_BUTTON
    };

    let show_view_button = real_set.is_ok();
    let set_name = set.name.cloned();

    let edit_name = editable && show_edit.cloned();

    rsx! {
        div {
            class: "border border-black p-4",
            div {
                class: "flex flex-row items-stretch",


                input {
                    class: "text-xl font-semibold mb-4 p-2 w-full bg-gray-100 rounded",
                    value: "{name}",
                    disabled: !edit_name,
                    oninput: move |evt|{
                        let val = evt.value();
                        name.set(val.to_string());
                    },
                }
                if save_button {
                    button {
                    class: "{save_class} h-[2.75rem]",

                    onclick: move |_| {
                        let expr: SetExpr = match SetExpr::try_from(set.expr.cloned()) {
                            Ok(t) => t,
                            Err(s) => {
                                dbg!(s);
                                return;
                            }
                        };

                        let name = name.cloned();
                        let id = set.id;


                        let event = SetEvent::new_modify(id, SetAction::SetName(name));
                        ledger.modify(event).unwrap();

                        let event = SetEvent::new_modify(id, SetAction::SetExpr(expr));
                        ledger.modify(event).unwrap();
                        set.name.write();

                    },
                    "save"
                }


                }

                button {
                    class: "{crate::styles::READ_BUTTON} h-[2.75rem]",
                    onclick: move |_| {
                        let expr: SetExpr = match SetExpr::try_from(set.expr.cloned()) {
                            Ok(t) => t,
                            Err(s) => {
                                dbg!(s);
                                return;
                            }
                        };


                        match reviewable_cards(expr, Some(filter2.clone())) {
                            Some(cards) => OverlayEnum::new_review(cards).append(),
                            None => OverlayEnum::new_notice("no cards to review!").append(),
                        }

                    },
                    "review"
                }

                if show_view_button {
                    button {
                        class: "{crate::styles::READ_BUTTON} h-[2.75rem]",
                        onclick: move |_|{
                            let expr = real_set.clone().unwrap();
                            dbg!(&expr);
                            let title = set_name.clone();
                            let viewer = CardSelector::new_with_filter(false, vec![], expr).with_title(title).with_edit_collection(false);
                            OverlayEnum::CardSelector(viewer).append();
                        },
                        "view"
                    }
                }

                if editable  {
                    button {
                        class: "{crate::styles::READ_BUTTON} h-[2.75rem]",
                        onclick: move |_|{
                            let flag = show_edit.cloned();
                            show_edit.clone().set(!flag);
                        },
                        "☰"
                    }
                }

                if editable && show_edit() {
                    button {
                        class: "{crate::styles::DELETE_BUTTON} h-[2.75rem]",
                        onclick: move |_|{
                            delete_atomic.set(true);

                            if !id.is_nil()  {
                                APP.read().inner().provider.sets.modify(TheLedgerEvent::new_delete(id)).unwrap();
                            }
                        },
                        "delete"
                    }
                }


            }
            if editable && show_edit() {
                RenderExpr { filter, inputs: set.expr.cloned().inputs.clone(), ty: set.expr.cloned().ty.clone(), depth: depth + 1}
            }
        }
    }
}

pub fn reviewable_cards(expr: SetExpr, filter: Option<CardFilter>) -> Option<NonEmpty<CardId>> {
    let provider = APP.read().inner().card_provider.clone();
    let cards = expr.eval(&provider);

    let card_ids: HashSet<CardId> = cards.iter().map(|card| card.id()).collect();
    let all_recursive_dependencies: HashSet<CardId> = card_ids
        .iter()
        .map(|id| APP.read().inner().provider.cards.all_dependencies(*id))
        .flatten()
        .collect();

    let mut cards_with_deps: BTreeSet<Arc<Card>> = Default::default();

    for (idx, card) in cards.into_iter().enumerate() {
        if idx % 50 == 0 {
            dbg!(idx);
        }

        let card = match card {
            MaybeCard::Card(card) => card,
            MaybeCard::Id(id) => provider.load(id).unwrap(),
        };

        cards_with_deps.insert(card);
    }

    for card in all_recursive_dependencies {
        let card = provider.load(card).unwrap();
        cards_with_deps.insert(card);
    }

    use rayon::prelude::*;

    let filtered_cards: Vec<Arc<Card>> = cards_with_deps
        .into_iter()
        .collect::<Vec<Arc<Card>>>()
        .into_par_iter()
        .filter(|card| {
            card.reviewable()
                && filter
                    .as_ref()
                    .map(|filter| filter.filter(card.clone()))
                    .unwrap_or(true)
        })
        .collect();

    let mut filtered_cards: Vec<CardId> =
        filtered_cards.into_iter().map(|card| card.id()).collect();

    use rand::seq::SliceRandom;
    filtered_cards.shuffle(&mut rand::thread_rng());

    NonEmpty::from_vec(filtered_cards)
}

#[derive(Debug, Clone, PartialEq)]
struct SetEditor {
    id: SetId,
    name: Signal<String>,
    expr: Signal<ExprEditor>,
    to_delete: Signal<bool>,
    show_edit: Signal<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputEditor {
    Leaf(DynCard),
    Card(CardId),
    Reference(SetId),
    Expr(ExprEditor),
}

impl Ord for InputEditor {
    fn cmp(&self, other: &Self) -> Ordering {
        use InputEditor::*;
        match (self, other) {
            (Card(a), Card(b)) => a.cmp(b),
            (Leaf(a), Leaf(b)) => a.cmp(b),
            (Reference(a), Reference(b)) => a.cmp(b),
            (Expr(a), Expr(b)) => a.cmp(&b),
            (Leaf(_), _) => Ordering::Less,
            (Card(_), Leaf(_)) => Ordering::Greater,
            (Card(_), _) => Ordering::Less,
            (Reference(_), Expr(_)) => Ordering::Less,
            (Reference(_), _) => Ordering::Greater,
            (Expr(_), _) => Ordering::Greater,
        }
    }
}

impl PartialOrd for InputEditor {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExprEditor {
    pub inputs: Signal<Vec<InputEditor>>,
    pub ty: Signal<SetExprDiscriminants>,
}

impl ExprEditor {
    pub fn expanded(&self) -> Memo<BTreeMap<Uuid, Signal<MaybeEntry>>> {
        info!("lets expand!");
        let selv = self.clone();
        ScopeId::APP.in_runtime(|| {
            let selv = selv.clone();
            Memo::new(move || {
                let selv = selv.clone();

                let res = match dbg!(SetExpr::try_from(selv)) {
                    Ok(expr) => {
                        let provider = APP.read().inner().card_provider.clone();
                        let mut out: BTreeMap<Uuid, Signal<MaybeEntry>> = Default::default();

                        for c in expr.eval(&provider) {
                            let id = c.id();
                            let entry = match c {
                                MaybeCard::Id(id) => MaybeEntry::No(id),
                                MaybeCard::Card(card) => MaybeEntry::Yes(card),
                            };

                            out.insert(id, Signal::new_in_scope(entry, ScopeId::APP));
                        }
                        out
                    }
                    Err(_) => Default::default(),
                };

                res
            })
        })
    }
}

impl PartialOrd for ExprEditor {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let self_inputs = self.inputs.read();
        let other_inputs = other.inputs.read();
        let self_ty = self.ty.read();
        let other_ty = other.ty.read();

        match self_ty.partial_cmp(&other_ty) {
            Some(Ordering::Equal) => self_inputs.partial_cmp(&other_inputs),
            non_eq => non_eq,
        }
    }
}

impl Ord for ExprEditor {
    fn cmp(&self, other: &Self) -> Ordering {
        let self_inputs = self.inputs.read();
        let other_inputs = other.inputs.read();
        let self_ty = self.ty.read();
        let other_ty = other.ty.read();

        match self_ty.cmp(&other_ty) {
            Ordering::Equal => self_inputs.cmp(&other_inputs),
            non_eq => non_eq,
        }
    }
}

impl From<SetExpr> for ExprEditor {
    fn from(value: SetExpr) -> Self {
        let inputs: Vec<InputEditor> = value
            .inputs()
            .into_iter()
            .cloned()
            .map(|input| InputEditor::from(input))
            .collect();

        let ty: SetExprDiscriminants = value.into();

        Self {
            inputs: Signal::new_in_scope(inputs, ScopeId::APP),
            ty: Signal::new_in_scope(ty, ScopeId::APP),
        }
    }
}

impl TryFrom<ExprEditor> for SetExpr {
    type Error = String;

    fn try_from(value: ExprEditor) -> Result<Self, Self::Error> {
        let mut inputs: BTreeSet<Input> = Default::default();

        for input in value.inputs.cloned() {
            let input: Input = Input::try_from(input)?;
            inputs.insert(input);
        }

        let ty = value.ty;

        match ty.cloned() {
            SetExprDiscriminants::Union => Ok(SetExpr::Union(inputs)),
            SetExprDiscriminants::Intersection => Ok(SetExpr::Intersection(inputs)),
            SetExprDiscriminants::Difference => {
                if inputs.len() != 2 {
                    Err("Difference takes exactly two elements".to_string())
                } else {
                    let mut iter = inputs.into_iter();
                    let elm1 = iter.next().unwrap();
                    let elm2 = iter.next().unwrap();

                    Ok(SetExpr::Difference(elm1, elm2))
                }
            }
            SetExprDiscriminants::All => {
                if !inputs.is_empty() {
                    Err("All takes no inputs".to_string())
                } else {
                    Ok(SetExpr::All)
                }
            }
            SetExprDiscriminants::Complement => {
                if inputs.len() != 1 {
                    Err("Complement takes exactly one element".to_string())
                } else {
                    let input = inputs.into_iter().next().unwrap();
                    Ok(SetExpr::Complement(input))
                }
            }
        }
    }
}

impl TryFrom<InputEditor> for Input {
    type Error = String;
    fn try_from(value: InputEditor) -> Result<Input, Self::Error> {
        Ok(match value {
            InputEditor::Leaf(dyn_card) => Input::Leaf(dyn_card),
            InputEditor::Reference(uuid) => Input::Reference(uuid),
            InputEditor::Expr(set_expr) => Input::Expr(Box::new(SetExpr::try_from(set_expr)?)),
            InputEditor::Card(id) => Input::Card(id),
        })
    }
}

impl From<Input> for InputEditor {
    fn from(value: Input) -> Self {
        match value {
            Input::Leaf(dyn_card) => InputEditor::Leaf(dyn_card),
            Input::Reference(uuid) => InputEditor::Reference(uuid),
            Input::Expr(set_expr) => InputEditor::Expr((*set_expr).into()),
            Input::Card(id) => InputEditor::Card(id),
        }
    }
}

impl SetEditor {
    fn new(set: &Set) -> Self {
        let show_edit = set.expr == SetExpr::default();

        Self {
            id: set.id,
            name: Signal::new_in_scope(set.name.clone(), ScopeId::APP),
            expr: Signal::new_in_scope(set.expr.clone().into(), ScopeId::APP),
            to_delete: Signal::new_in_scope(false, ScopeId::APP),
            show_edit: Signal::new_in_scope(show_edit, ScopeId::APP),
        }
    }
}

#[component]
fn RenderSets(filter: CardFilter, sets: Signal<Vec<SetEditor>>) -> Element {
    let all_set: SetEditor = SetEditor::new(&Set {
        id: Uuid::nil(),
        name: "all cards".to_string(),
        expr: SetExpr::All,
    });

    let to_delete = sets.iter().position(|set| set.to_delete.cloned());

    if let Some(idx) = to_delete {
        sets.write().remove(idx);
        ScopeId::APP.needs_update();
    }

    let the_sets = sets.clone();

    rsx! {
        div {
            class: "overflow-y-auto max-h-[80vh] space-y-2 pr-2",
            RenderSet { filter: filter.clone(), set: all_set, editable: false}
            for set in sets.cloned() {
                RenderSet { filter: filter.clone(), set}
            }

            button {
                class: "{styles::CREATE_BUTTON}",
                onclick: move |_|{
                    let f: Arc<Box<dyn Fn(String)>> = Arc::new(Box::new(move |s: String| {
                        let event = SetEvent::new_modify(SetId::new_v4(), SetAction::SetName(s));
                        APP.read().inner().provider.sets.modify(event).unwrap();
                        the_sets.clone().set(load_sets());
                    }));
                    OverlayEnum::new_text_input("name of set".to_string(), f).append();
                },
                "new set"
            }
        }
    }
}
