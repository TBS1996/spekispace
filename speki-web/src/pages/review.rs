use std::{cmp::Ordering, collections::BTreeSet, fmt::Debug, sync::Arc};

use dioxus::prelude::*;
use either::IntoEither;
use ledgerstore::LedgerItem;
use serde::{Deserialize, Serialize};
use speki_core::{
    card::CardId,
    cardfilter::CardFilter,
    collection::{Collection, DynCard, MaybeCard},
    ledger::{CollectionAction, CollectionEvent},
    set::{Input, Set, SetAction, SetEvent, SetExpr, SetExprDiscriminants, SetId},
    Card,
};
use strum::{Display, EnumDiscriminants, EnumIter, EnumString, IntoEnumIterator};
use tracing::info;
use uuid::Uuid;

use crate::{
    components::{
        dropdown::{Displayer, DropComponent},
        DropDownMenu, FilterComp, FilterEditor,
    },
    overlays::{
        card_selector::{CardSelector, MyClosure},
        colviewer::CollectionEditor,
        reviewsession::{ReviewSession, ReviewState},
        textinput::TextInput,
        Overender, OverlayChoice, OverlayEnum, OverlaySelector,
    },
    APP,
};

#[derive(Clone)]
pub struct ReviewPage {
    filter: FilterEditor,
    cardfilter: Memo<CardFilter>,
    collections: Signal<Vec<(Collection, RecallDist)>>,
    overlay: Signal<Option<OverlayEnum>>,
}

impl ReviewPage {
    pub fn new() -> Self {
        let filter = FilterEditor::new_default();
        let cardfilter = filter.memo();
        let selv = Self {
            filter,
            cardfilter,
            collections: Default::default(),
            overlay: Default::default(),
        };

        let cols = selv.collections.clone();

        spawn(async move {
            let _cols = APP.read().load_collections().await;
            let mut out = vec![];

            for col in _cols.clone() {
                out.push((col, RecallDist::default()));
            }
            cols.clone().set(out);
            return;

            let mut out = vec![];

            let mut futs = vec![];

            for col in _cols {
                futs.push(async move {
                    let dist = RecallDist::new(col.clone()).await;
                    (col, dist)
                });
            }

            for (col, dist) in futures::future::join_all(futs).await {
                out.push((col, dist));
            }

            cols.clone().set(out);
        });

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
        let sets: Vec<SetEditor> = APP
            .read()
            .inner()
            .provider
            .sets
            .load_all()
            .into_values()
            .map(|set| SetEditor::new(&set))
            .collect();

        Signal::new_in_scope(sets, ScopeId::APP)
    };

    rsx! {
        Overender {
            overlay,
            root: rsx!{
                div {
                    class: "flex flex-row items-start min-h-screen space-y-4 justify-start w-full",
                    FilterComp {editor}
                    /*
                    RenderCols{
                        filter: state.filter.to_filter(),
                        collections: state.collections.clone(),
                        overlay: state.overlay.clone(),
                    }
                    */
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

    async fn new(col: Collection) -> Self {
        let mut selv = Self::default();
        //return selv;

        for card in col.expand(APP.read().inner().card_provider()).await {
            *match card.recall_rate() {
                Some(rate) => {
                    if rate < 0.05 {
                        &mut selv.n1
                    } else if rate < 0.2 {
                        &mut selv.n2
                    } else if rate < 0.5 {
                        &mut selv.n3
                    } else if rate < 0.8 {
                        &mut selv.n4
                    } else if rate < 0.95 {
                        &mut selv.n5
                    } else {
                        &mut selv.n6
                    }
                }
                None => &mut selv.p,
            } += 1;
        }

        tracing::info!("{selv:?}");

        selv
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
fn RenderExpr(
    filter: CardFilter,
    inputs: Signal<BTreeSet<InputEditor>>,
    ty: Signal<SetExprDiscriminants>,
    overlay: Signal<Option<OverlayEnum>>,
    #[props(default = 0)] depth: usize,
) -> Element {
    let class = format!("pl-{}", depth * 4);

    dbg!(&class);

    rsx! {
        div {
            class: "{class}",

            div {
                class: "flex flex-row",
                DropComponent { options: SetExprDiscriminants::iter().collect(), selected: ty}

                button {
                    onclick: move |_|{
                        let expr = SetExpr::default();
                        let input: InputEditor = Input::Expr(expr.into()).into();
                        inputs.clone().write().insert(input);
                    },
                    "expr"
                }

                button {
                    onclick: move |_| {
                        // normal card
                        let leaf_card = {
                            let f: Arc<Box<dyn Fn() -> OverlayEnum>> = {
                                let f = move || {
                                    let f = move |card: Signal<Card>| {
                                        let leaf = DynCard::Card(card.read().id());
                                        let input = InputEditor::Leaf(leaf);
                                        inputs.clone().write().insert(input);
                                        async move {}
                                    };
                                    let overlay = CardSelector::new(false, vec![]).new_on_card_selected(MyClosure::new(f));
                                    OverlayEnum::CardSelector(overlay)
                                };

                                Arc::new(Box::new(f))
                            };

                            OverlayChoice { display: "Add card".to_string(), overlay: f }
                        };


                        let instances = {
                            let f: Arc<Box<dyn Fn() -> OverlayEnum>> = {
                                let f = move || {
                                    let f = move |card: Signal<Card>| {
                                        let leaf = DynCard::Instances(card.read().id());
                                        let input = InputEditor::Leaf(leaf);
                                        inputs.clone().write().insert(input);
                                        async move {}
                                    };
                                    let overlay = CardSelector::class_picker(MyClosure::new(f));
                                    OverlayEnum::CardSelector(overlay)
                                };

                                Arc::new(Box::new(f))
                            };

                            OverlayChoice { display: "instances".to_string(), overlay: f }
                        };


                        let dependents = {
                            let f: Arc<Box<dyn Fn() -> OverlayEnum>> = {
                                let f = move || {
                                    let f = move |card: Signal<Card>| {
                                        let leaf = DynCard::RecDependents(card.read().id());
                                        let input = InputEditor::Leaf(leaf);
                                        inputs.clone().write().insert(input);
                                        async move {}
                                    };
                                    let overlay = CardSelector::new(false, vec![]).new_on_card_selected(MyClosure::new(f));
                                    OverlayEnum::CardSelector(overlay)
                                };

                                Arc::new(Box::new(f))
                            };

                            OverlayChoice { display: "dependents".to_string(), overlay: f }
                        };

                        let sel = OverlaySelector { title: "dyn ty".to_string(), choices: vec![leaf_card, instances, dependents], chosen: None};
                        overlay.clone().set(Some(OverlayEnum::OverlaySelector(sel)));

                    },
                    "leaf"
                }
            }



            for input in inputs.cloned() {
                div {
                    class: "flex flex-row items-start {class}",
                    button {
                        class: "mt-1", // optional: fine-tune vertical alignment
                        onclick: move |_| {
                            assert!(inputs.write().remove(&input));
                        },
                        "X"
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
) -> Element {
    let indent = format!("{}⎯ ", " ".repeat(depth));
    let mut name = set.name.clone();

    let ledger = APP.read().inner().provider.sets.clone();
    let filter2 = filter.clone();

    rsx! {
        div {
            div {
                class: "flex flex-row",
                input {
                    value: "{name}",
                    oninput: move |evt|{
                        let val = evt.value();
                        name.set(val.to_string());
                    },
                }
                button {
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

                button {
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

                            let mut filtered_cards: Vec<CardId> = vec![];


                            for card in cards {
                                let card: Arc<Card> = match card {
                                    MaybeCard::Card(c) => c,
                                    MaybeCard::Id(id) => {
                                        provider.load(id).unwrap()
                                    },
                                };

                                let id = card.id();
                                if filter.filter(card).await {
                                    filtered_cards.push(id);
                                }
                            }

                            let revses = OverlayEnum::Review(ReviewState::new(filtered_cards));
                            overlay.clone().set(Some(revses));


                        });


                    },
                    "review"
                }
            }
            RenderExpr { filter, inputs: set.expr.cloned().inputs.clone(), ty: set.expr.cloned().ty.clone(), depth: depth + 1 , overlay}
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
    pub inputs: Signal<BTreeSet<InputEditor>>,
    pub ty: Signal<SetExprDiscriminants>,
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
        let inputs: BTreeSet<InputEditor> = value
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
    rsx! {
        div {
        class: "flex flex-col mb-10",
        for set in sets.cloned() {
            RenderSet { filter: filter.clone(), set , overlay}
        }

        button {
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

#[component]
fn RenderCols(
    filter: CardFilter,
    collections: Signal<Vec<(Collection, RecallDist)>>,
    overlay: Signal<Option<OverlayEnum>>,
) -> Element {
    let mut colfil: Vec<(Collection, RecallDist, CardFilter)> = vec![];

    for (col, dist) in collections.cloned() {
        colfil.push((col, dist, filter.clone()));
    }

    rsx! {
        div {
       //     class: "flex flex-col max-w-[550px] mr-5",

            div {
                button {
                    class: "inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base mb-8",
                    onclick: move |_| {
                        let filter = filter.clone();
                        spawn(async move {
                            let session = ReviewSession::new(vec![DynCard::Any], filter).await;
                            let cards: Vec<CardId> = session.expand().await.into_iter().map(|c|c.id()).collect();
                            let revses = OverlayEnum::Review(ReviewState::new(cards));
                            overlay.clone().set(Some(revses));
                        });
                    },
                    "review all"
                    }

                button {
                    class: "inline-flex items-center text-white bg-blue-700 border-0 py-1 px-3 focus:outline-none hover:bg-blue-900 rounded text-base mb-5",
                    onclick: move |_| {
                        let done = Signal::new_in_scope(false, ScopeId::APP);
                        let f = move |name: String| {
                            info!("new collection made!");
                            spawn(async move {
                                info!("saving it!");
                                let event = CollectionEvent::new(Uuid::new_v4(), CollectionAction::SetName(name));
                                APP.read().inner().provider.run_event(event);
                                done.clone().set(true);
                                info!("saved it!");
                            });
                            info!("bye");
                        };

                        let txt = OverlayEnum::Text(TextInput::new("add collection".to_string(), Arc::new(Box::new(f)), done));
                        overlay.clone().set(Some(txt));
                    },
                    "add collection"
                }
            }

            for (col, dist, filter) in colfil {
                div {
                    class: "flex flex-col mb-4",
                    div {
                    class: "flex flex-row",
                        button {
                            class: "inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base mb-2",
                            onclick: move |_| {
                                let filter = filter.clone();
                                spawn(async move {
                                    let col = APP.read().load_collection(col.id).await;
                                    let provider = APP.read().inner().card_provider.clone();
                                    let mut cards = vec![];
                                    for card in col.dyncards {
                                        cards.extend(card.expand(provider.clone(), Default::default()).await);
                                    }
                                    let session = OverlayEnum::Review(ReviewState::new_with_filter(cards, filter).await);
                                    overlay.clone().set(Some(session));
                                });
                            },
                            "{col.name}"
                        }
                        button {
                            class: "ml-auto inline-flex items-center text-white bg-blue-700 border-0 py-1 px-3 focus:outline-none hover:bg-blue-900 rounded text-base mb-5",
                            onclick: move |_|{
                                spawn(async move {
                                    let viewer = OverlayEnum::Colviewer(CollectionEditor::new(col.id).await);
                                    overlay.clone().set(Some(viewer));
                                });
                            },
                            "edit"
                        }
                    }

                    RecallBar { dist  }
                }
            }

        }
    }
}
