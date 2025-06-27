use crate::{
    append_overlay,
    components::{
        dropdown::{ActionDropdown, DropComponent, DropdownAction, DropdownClosure},
        FilterComp, FilterEditor,
    },
    overlays::{
        card_selector::{CardSelector, MaybeEntry, MyClosure},
        reviewsession::ReviewState,
        Overender, OverlayChoice, OverlayEnum, OverlaySelector,
    },
    APP,
};
use crate::{styles, OVERLAY};
use dioxus::prelude::*;
use ledgerstore::{LedgerItem, TheLedgerEvent};
use speki_core::card::CType;
use speki_core::cardfilter::MyNumOrd;
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

impl ReviewPage {
    pub fn new() -> Self {
        let filter = FilterEditor::new_default();
        let cardfilter = filter.memo();

        let sets: Signal<Vec<SetEditor>> = {
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

            Signal::new_in_scope(sets, ScopeId::APP)
        };

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
                    class: "flex flex-row items-start min-h-screen space-y-4 justify-start w-full",
                    FilterComp {editor}
                    RenderSets {filter: state.filter.to_filter(), sets }
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
                .unwrap()
                .name()
                .to_string();
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

    let expr_func: DropdownClosure = Arc::new(Box::new(move || {
        let expr = SetExpr::default();
        let input: InputEditor = Input::Expr(expr.into()).into();
        inputs.clone().write().push(input);
    }));

    let leaf_func: DropdownClosure = Arc::new(Box::new(move || {
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
            ],
            chosen: None,
        };
        append_overlay(OverlayEnum::OverlaySelector(sel));
    }));

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
    let mut name = set.name.clone();
    let id = set.id;
    let mut delete_atomic = set.to_delete.clone();

    let ledger = APP.read().inner().provider.sets.clone();
    let filter2 = filter.clone();

    let real_set = SetExpr::try_from(set.expr.cloned());

    let save_button: bool = match real_set.clone() {
        Ok(set_expr) => match APP.read().inner().provider.sets.try_load(set.id) {
            Some(old_set) => old_set.expr != set_expr || old_set.name != set.name.cloned(),
            None => true,
        },
        Err(_) => false,
    } && editable;

    let show_view_button = real_set.is_ok();
    let set_name = set.name.cloned();

    rsx! {
        div {
            class: "border border-black p-4",
            div {
                class: "flex flex-row",

                input {
                    class: "text-xl font-semibold mb-4 p-2 w-full bg-gray-100 rounded",
                    value: "{name}",
                    disabled: !editable,
                    oninput: move |evt|{
                        let val = evt.value();
                        name.set(val.to_string());
                    },
                }
                if save_button {
                    button {
                    class: "{crate::styles::BLACK_BUTTON}",

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
                    class: "{crate::styles::BLACK_BUTTON}",
                    onclick: move |_| {
                            dbg!();
                        let expr: SetExpr = match SetExpr::try_from(set.expr.cloned()) {
                            Ok(t) => t,
                            Err(s) => {
                                dbg!(s);
                                return;
                            }
                        };

                        let filter = filter2.clone();
                        dbg!();
                        let provider = APP.read().inner().card_provider.clone();
                        dbg!();
                        let cards = expr.eval(&provider);
                        dbg!();

                        let card_ids: HashSet<CardId> = cards.iter().map(|card|card.id()).collect();
                        dbg!();
                        let all_recursive_dependencies: HashSet<CardId> = card_ids.iter().map(|id|APP.read().inner().provider.cards.recursive_dependencies(*id)).flatten().collect();
                        dbg!(cards.len());

                        let mut cards_with_deps: BTreeSet<Arc<Card>> = Default::default();

                        for (idx, card) in cards.into_iter().enumerate() {
                            if idx % 50 == 0 {
                                dbg!(idx);
                            }

                            let card = match card {
                                MaybeCard::Card(card) => card,
                                MaybeCard::Id(id) => {
                                    provider.load(id).unwrap()
                                },
                            };

                            cards_with_deps.insert(card);
                        }

                        dbg!();

                        for card in all_recursive_dependencies {
                            let card = provider.load(card).unwrap();
                            cards_with_deps.insert(card);
                        }



                        dbg!();


                        use rayon::prelude::*;


                        let filtered_cards: Vec<Arc<Card>> = cards_with_deps
                            .iter()
                            .cloned()
                            .collect::<Vec<Arc<Card>>>()
                            .into_par_iter()
                            .filter(|card| filter.filter(card.clone()))
                            .collect();

                        dbg!();


                        if let Some(recall) = filter.recall {
                            if recall.ord == MyNumOrd::Less {
                                //filtered_cards.retain(|card| card.full_recall_rate().unwrap_or_default() < recall.num);
                            }
                        }

                        let mut filtered_cards: Vec<CardId> = filtered_cards.into_iter().map(|card|card.id()).collect();
                        dbg!();


                        use rand::seq::SliceRandom;
                        filtered_cards.shuffle(&mut rand::thread_rng());

                        dbg!();



                        let revses = OverlayEnum::Review(ReviewState::new(filtered_cards));
                        append_overlay(revses);
                    },
                    "review"
                }

                /*
                button {
                    class: "{crate::styles::BLACK_BUTTON}",
                    onclick: move |_| {
                        let name = set.name.cloned();
                        let expr: SetExpr = match SetExpr::try_from(set.expr.cloned()) {
                            Ok(t) => t,
                            Err(s) => {
                                dbg!(s);
                                return;
                            }
                        };

                        let provider = APP.read().inner().card_provider.clone();
                        let mcards = expr.eval(&provider);

                        let mut cards: BTreeSet<Arc<Card>> = BTreeSet::new();

                        for card in mcards {
                            let card = match card {
                                MaybeCard::Card(card) => card,
                                MaybeCard::Id(id) => {
                                 provider.load(id).unwrap()
                                },
                            };

                            for dep in card.recursive_dependencies() {
                                let card = provider.load(dep).unwrap();
                                cards.insert(card);
                            }
                            cards.insert(card);
                        }

                        let dot = speki_core::graphviz::export_cards(cards);
                        let mut path = PathBuf::from(name);
                        path.set_extension("dot");
                        let mut f = fs::File::create(&path).unwrap();
                        f.write_all(dot.as_bytes()).unwrap();
                        info!("done exporting to {path:?}!");

                    },
                    "export DOT"
                }
                */

                if show_view_button {
                    button {
                        class: "{crate::styles::BLACK_BUTTON}",
                        onclick: move |_|{
                            let expr = real_set.clone().unwrap();
                            dbg!(&expr);
                            let title = set_name.clone();
                            let viewer = CardSelector::new_with_filter(false, vec![], expr).with_title(title).with_edit_collection(false);
                            append_overlay(OverlayEnum::CardSelector(viewer));
                        },
                        "view"
                    }
                }

                if editable {
                    button {
                        class: "{crate::styles::BLACK_BUTTON}",
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
            if editable {
                RenderExpr { filter, inputs: set.expr.cloned().inputs.clone(), ty: set.expr.cloned().ty.clone(), depth: depth + 1}
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct SetEditor {
    id: SetId,
    name: Signal<String>,
    expr: Signal<ExprEditor>,
    to_delete: Signal<bool>,
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
            use_memo(move || {
                let selv = selv.clone();

                let res = match dbg!(SetExpr::try_from(selv)) {
                    Ok(expr) => {
                        let provider = APP.read().inner().card_provider.clone();
                        let mut out: BTreeMap<Uuid, Signal<MaybeEntry>> = Default::default();

                        for c in expr.eval(&provider) {
                            let id = c.id();
                            let entry = match c {
                                MaybeCard::Id(id) => MaybeEntry::No(id),
                                MaybeCard::Card(card) => MaybeEntry::Yes(Signal::new_in_scope(
                                    Arc::unwrap_or_clone(card),
                                    ScopeId::APP,
                                )),
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
        Self {
            id: set.id,
            name: Signal::new_in_scope(set.name.clone(), ScopeId::APP),
            expr: Signal::new_in_scope(set.expr.clone().into(), ScopeId::APP),
            to_delete: Signal::new_in_scope(false, ScopeId::APP),
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

    rsx! {
        div {
            class: "overflow-y-auto max-h-[80vh] space-y-2 pr-2",
            RenderSet { filter: filter.clone(), set: all_set, editable: false}
            for set in sets.cloned() {
                RenderSet { filter: filter.clone(), set}
            }

            button {
                class: "{styles::BLUE_BUTTON}",
                onclick: move |_|{
                    let set = Set::new_default(SetId::new_v4());
                    let set = SetEditor::new(&set);
                    sets.clone().write().push(set);
                },
                "new set"
            }
        }
    }
}
