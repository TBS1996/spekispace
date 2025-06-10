use crate::styles;
use crate::{
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
use dioxus::prelude::*;
use ledgerstore::LedgerItem;
use speki_core::{
    card::CardId,
    cardfilter::CardFilter,
    collection::{DynCard, MaybeCard},
    set::{Input, Set, SetAction, SetEvent, SetExpr, SetExprDiscriminants, SetId},
    Card,
};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    fmt::Debug,
    fs,
    io::Write,
    path::PathBuf,
    sync::Arc,
};
use strum::IntoEnumIterator;
use tracing::info;
use uuid::Uuid;

#[derive(Clone)]
pub struct ReviewPage {
    filter: FilterEditor,
    cardfilter: Memo<CardFilter>,
    overlay: Signal<Option<OverlayEnum>>,
}

impl ReviewPage {
    pub fn new() -> Self {
        let filter = FilterEditor::new_default();
        let cardfilter = filter.memo();
        let selv = Self {
            filter,
            cardfilter,
            overlay: Default::default(),
        };

        selv
    }
}

#[component]
pub fn Review() -> Element {
    let state: ReviewPage = use_context::<ReviewPage>();
    let editor = state.filter.clone();
    tracing::info!("memo lol: {:?}", &state.cardfilter);

    let overlay = state.overlay.clone();
    let sets: Signal<Vec<SetEditor>> = {
        let mut sets: Vec<SetEditor> = APP
            .read()
            .inner()
            .provider
            .sets
            .load_all()
            .into_values()
            .map(|set| SetEditor::new(&set))
            .collect();

        sets.sort_by_key(|set| set.name.cloned());

        Signal::new_in_scope(sets, ScopeId::APP)
    };

    rsx! {
        Overender {
            overlay,
            root: rsx!{
                div {
                    class: "flex flex-row items-start min-h-screen space-y-4 justify-start w-full",
                    FilterComp {editor}
                    RenderSets {filter: state.filter.to_filter(), sets, overlay }
                }
            }
        }
    }
}

#[component]
fn RecallBar(dist: RecallDist) -> Element {
    let proportions = dist.proportions();

    rsx!(
        div {
            class: "flex w-full h-4 rounded overflow-hidden border border-gray-300",
            for (percentage, color) in proportions {
                div {
                    style: format!(
                        "width: {}%; background-color: {};",
                        percentage,
                        color
                    ),
                    key: "{color}",
                }
            }
        }
    )
}

#[derive(Default, Clone, PartialEq, Debug)]
struct RecallDist {
    p: u32,
    n1: u32,
    n2: u32,
    n3: u32,
    n4: u32,
    n5: u32,
    n6: u32,
}

impl RecallDist {
    const HEXP: &str = "#00FFFF";
    const HEX1: &str = "#FF0D0D";
    const HEX2: &str = "#FF4E11";
    const HEX3: &str = "#FF8E15";
    const HEX4: &str = "#FAB733";
    const HEX5: &str = "#ACB334";
    const HEX6: &str = "#69B34C";

    fn total(&self) -> u32 {
        self.p + self.n1 + self.n2 + self.n3 + self.n4 + self.n5 + self.n6
    }

    fn proportions(&self) -> Vec<(f32, &'static str)> {
        let total = self.total();
        if total == 0 {
            return vec![];
        }
        vec![
            (self.p as f32 / total as f32 * 100.0, Self::HEXP),
            (self.n1 as f32 / total as f32 * 100.0, Self::HEX1),
            (self.n2 as f32 / total as f32 * 100.0, Self::HEX2),
            (self.n3 as f32 / total as f32 * 100.0, Self::HEX3),
            (self.n4 as f32 / total as f32 * 100.0, Self::HEX4),
            (self.n5 as f32 / total as f32 * 100.0, Self::HEX5),
            (self.n6 as f32 / total as f32 * 100.0, Self::HEX6),
        ]
    }
}

#[component]
fn RenderInput(
    filter: CardFilter,
    input: InputEditor,
    #[props(default = 0)] depth: usize,
    overlay: Signal<Option<OverlayEnum>>,
) -> Element {
    let ledger = APP.read().inner().provider.sets.clone();
    let indent = format!("{}• ", " ".repeat(depth));

    let leaf = match input {
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
            InputEditor::Leaf(_) => {
                rsx!{
                    p { "{leaf}" }}
            },
            InputEditor::Reference(id) => {
                rsx!{RenderSet { filter, set: SetEditor::new(&ledger.load(id).unwrap()), depth: depth + 1, overlay}}
            },
            InputEditor::Expr(expr) => {
                rsx!{

                    RenderExpr {filter, inputs: expr.inputs.clone(), ty: expr.ty.clone(), depth: depth + 1, overlay}
                }
            },
        }
    }
    }
}

#[component]
pub fn RenderExpr(
    filter: CardFilter,
    inputs: Signal<Vec<InputEditor>>,
    ty: Signal<SetExprDiscriminants>,
    overlay: Signal<Option<OverlayEnum>>,
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
            let f: Arc<Box<dyn Fn() -> OverlayEnum>> = {
                let f = move || {
                    let f = move |card: Signal<Card>| {
                        let leaf = DynCard::Card(card.read().id());
                        let input = InputEditor::Leaf(leaf);
                        inputs.clone().write().push(input);
                        async move {}
                    };
                    let overlay =
                        CardSelector::new(false, vec![]).new_on_card_selected(MyClosure::new(f));
                    OverlayEnum::CardSelector(overlay)
                };

                Arc::new(Box::new(f))
            };

            OverlayChoice {
                display: "Add card".to_string(),
                overlay: f,
            }
        };

        let instances = {
            let f: Arc<Box<dyn Fn() -> OverlayEnum>> = {
                let f = move || {
                    let f = move |card: Signal<Card>| {
                        let leaf = DynCard::Instances(card.read().id());
                        let input = InputEditor::Leaf(leaf);
                        inputs.clone().write().push(input);
                        async move {}
                    };
                    let overlay = CardSelector::class_picker(MyClosure::new(f));
                    OverlayEnum::CardSelector(overlay)
                };

                Arc::new(Box::new(f))
            };

            OverlayChoice {
                display: "instances".to_string(),
                overlay: f,
            }
        };

        let dependents = {
            let f: Arc<Box<dyn Fn() -> OverlayEnum>> = {
                let f = move || {
                    let f = move |card: Signal<Card>| {
                        let leaf = DynCard::RecDependents(card.read().id());
                        let input = InputEditor::Leaf(leaf);
                        inputs.clone().write().push(input);
                        async move {}
                    };
                    let overlay =
                        CardSelector::new(false, vec![]).new_on_card_selected(MyClosure::new(f));
                    OverlayEnum::CardSelector(overlay)
                };

                Arc::new(Box::new(f))
            };

            OverlayChoice {
                display: "dependents".to_string(),
                overlay: f,
            }
        };

        let sel = OverlaySelector {
            title: "dyn ty".to_string(),
            choices: vec![leaf_card, instances, dependents],
            chosen: None,
        };
        overlay.clone().set(Some(OverlayEnum::OverlaySelector(sel)));
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
                        "❌"
                    }
                    RenderInput { filter: filter.clone(), input: input.clone(), depth: depth + 1, overlay }
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
    overlay: Signal<Option<OverlayEnum>>,
    #[props(default = true)] editable: bool,
) -> Element {
    let mut name = set.name.clone();

    let ledger = APP.read().inner().provider.sets.clone();
    let filter2 = filter.clone();

    let real_set = SetExpr::try_from(set.expr.cloned());

    let save_button: bool = match real_set.clone() {
        Ok(set_expr) => match APP.read().inner().provider.sets.load(set.id) {
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


                        let event = SetEvent::new(id, SetAction::SetName(name));
                        ledger.insert_ledger(event);

                        let event = SetEvent::new(id, SetAction::SetExpr(expr));
                        ledger.insert_ledger(event);

                    },
                    "save"
                }


                }

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

                        let filter = filter2.clone();
                        spawn(async move {
                            let provider = APP.read().inner().card_provider.clone();
                            let cards = expr.eval(&provider);

                            let mut cards_with_deps: BTreeSet<Arc<Card>> = Default::default();

                            for card in cards {
                                let card = match card {
                                    MaybeCard::Card(card) => card,
                                    MaybeCard::Id(id) => {
                                    provider.load(id).unwrap()
                                    },
                                };

                                for dep_card in card.recursive_dependencies_as_card(){
                                    cards_with_deps.insert(dep_card);
                                }

                                cards_with_deps.insert(card);
                            }

                            let mut filtered_cards: Vec<CardId> = vec![];


                            for card in cards_with_deps {
                                let id = card.id();
                                if filter.filter(card).await {
                                    filtered_cards.push(id);
                                }
                            }

                            use rand::seq::SliceRandom;
                            filtered_cards.shuffle(&mut rand::thread_rng());


                            let revses = OverlayEnum::Review(ReviewState::new(filtered_cards));
                            overlay.clone().set(Some(revses));


                        });


                    },
                    "review"
                }

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

                if show_view_button {
                    button {
                        class: "{crate::styles::BLACK_BUTTON}",
                        onclick: move |_|{
                            let expr = real_set.clone().unwrap();
                            let title = set_name.clone();
                            let viewer = CardSelector::new(false, vec![]).with_set(expr).with_title(title).with_edit_collection(false);
                            overlay.set(Some(OverlayEnum::CardSelector(viewer)));
                        },
                        "view"
                    }
                }
            }
            if editable {
                RenderExpr { filter, inputs: set.expr.cloned().inputs.clone(), ty: set.expr.cloned().ty.clone(), depth: depth + 1 , overlay}
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct SetEditor {
    id: SetId,
    name: Signal<String>,
    expr: Signal<ExprEditor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputEditor {
    Leaf(DynCard),
    Reference(SetId),
    Expr(ExprEditor),
}

impl Ord for InputEditor {
    fn cmp(&self, other: &Self) -> Ordering {
        use InputEditor::*;
        match (self, other) {
            (Leaf(a), Leaf(b)) => a.cmp(b),
            (Reference(a), Reference(b)) => a.cmp(b),
            (Expr(a), Expr(b)) => a.cmp(&b),
            (Leaf(_), _) => Ordering::Less,
            (Reference(_), Leaf(_)) => Ordering::Greater,
            (Reference(_), Expr(_)) => Ordering::Less,
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
    pub fn expanded(&self) -> Resource<BTreeMap<Uuid, Signal<MaybeEntry>>> {
        info!("lets expand!");
        let selv = self.clone();
        ScopeId::APP.in_runtime(|| {
            let selv = selv.clone();
            use_resource(move || {
                let selv = selv.clone();

                let res = match SetExpr::try_from(selv) {
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

                async move { res }
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
        })
    }
}

impl From<Input> for InputEditor {
    fn from(value: Input) -> Self {
        match value {
            Input::Leaf(dyn_card) => InputEditor::Leaf(dyn_card),
            Input::Reference(uuid) => InputEditor::Reference(uuid),
            Input::Expr(set_expr) => InputEditor::Expr((*set_expr).into()),
        }
    }
}

impl SetEditor {
    fn new(set: &Set) -> Self {
        Self {
            id: set.id,
            name: Signal::new_in_scope(set.name.clone(), ScopeId::APP),
            expr: Signal::new_in_scope(set.expr.clone().into(), ScopeId::APP),
        }
    }
}

#[component]
fn RenderSets(
    filter: CardFilter,
    sets: Signal<Vec<SetEditor>>,
    overlay: Signal<Option<OverlayEnum>>,
) -> Element {
    let all_set: SetEditor = SetEditor::new(&Set {
        id: Uuid::nil(),
        name: "all cards".to_string(),
        expr: SetExpr::All,
    });

    rsx! {
        div {
            class: "flex flex-col mb-10",
            RenderSet { filter: filter.clone(), set: all_set , overlay, editable: false}
            for set in sets.cloned() {
                RenderSet { filter: filter.clone(), set , overlay}
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
