use dioxus::prelude::*;
use either::Either;
use ledgerstore::TheLedgerEvent;
use nonempty::NonEmpty;
use std::{collections::BTreeSet, rc::Rc, sync::Arc};

use speki_core::{
    card::{CardId, EvalText},
    cardfilter::CardFilter,
    collection::DynCard,
    ledger::CardAction,
    recall_rate::Recall,
    set::{Input, SetExpr},
    Card,
};
use tracing::info;

use crate::{
    components::{card_mastery::MasterySection, RenderDependents},
    overlays::{
        card_selector::{CardSelector, MyClosure},
        cardviewer::{AdderHeader, CardViewer},
    },
    pop_overlay,
    utils::recall_to_emoji,
    APP,
};

use super::OverlayEnum;

#[component]
fn RecallButton(
    recall: Recall,
    card: Signal<Card>,
    mut queue: Signal<Queue>,
    mut show_backside: Signal<bool>,
) -> Element {
    let label = recall_to_emoji(recall);
    let title = recall.desc();

    rsx! {
        button {
            class: "bg-blue-500 mt-6 inline-flex items-center justify-center text-white border-0 py-4 px-6 focus:outline-none hover:bg-blue-700 rounded text-4xl leading-none font-emoji",
            title,
            onclick: move |_| {
                let mut card = card.clone();
                info!("do review");
                card.write()
                    .add_review(recall);
                queue.write().next();
                show_backside.set(false);


            },
            "{label}"

        }
    }
}

#[component]
fn ReviewButtons(
    mut show_backside: Signal<bool>,
    card: Signal<Card>,
    queue: Signal<Queue>,
) -> Element {
    rsx! {
        div {
            class: "flex flex-col items-center justify-center h-[680px]",

            if !show_backside() {
                button {
                    class: "{crate::styles::READ_BUTTON}",
                    onclick: move |_| {

                        show_backside.set(true);
                    },
                    "show backside"
                }
            } else {
                div {
                    class: "flex gap-4 justify-center items-center",

                    for recall in [Recall::None, Recall::Late, Recall::Some, Recall::Perfect] {
                         RecallButton {
                            recall,
                            card: card.clone(),
                            queue: queue.clone(),
                            show_backside: show_backside.clone(),
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct ReviewSession {
    cards: Vec<DynCard>,
    filter: CardFilter,
    thecards: BTreeSet<Arc<Card>>,
}

#[component]
pub fn ReviewRender(
    queue: Signal<Queue>,
    card_id: CardId,
    show_backside: Signal<bool>,
    tot: Memo<usize>,
) -> Element {
    let card = match APP.read().inner().card_provider.load(card_id) {
        Some(card) => card,
        None => {
            queue.write().next();
            return rsx! {};
        }
    };

    let explicit_dependencies: Vec<Arc<Card>> = {
        let mut deps: Vec<Arc<Card>> = vec![];

        for dep in &card.explicit_dependencies() {
            let dep = APP.read().load_card(*dep);
            deps.push(dep);
        }
        deps
    };

    let history = card.history().to_owned();

    let front = card.front_side().to_owned();
    let back = card.backside().to_owned();

    let card2 = card.clone();
    let log_event = move |event: Rc<KeyboardData>| {
        let card = card2.clone();
        info!("reviewing..");
        let bck = show_backside.cloned();
        let recall = match event.key().to_string().as_str() {
            "1" if bck => Recall::None,
            "2" if bck => Recall::Late,
            "3" if bck => Recall::Some,
            "4" if bck => Recall::Perfect,
            " " => {
                show_backside.clone().set(true);

                return;
            }
            _ => return,
        };
        queue.clone().write().next();
        show_backside.clone().set(false);
        let mut card = Arc::unwrap_or_clone(card);
        card.add_review(recall);
    };

    rsx! {
                div {
                    class: "h-full w-full flex flex-col",
                    id: "receiver",
                    tabindex: 0,
                    onkeydown: move |event| log_event(event.data()),

                    div {
                        class: "flex-none w-full",
                        Infobar {
                            card: card.clone(),
                            tot,
                            queue: queue.clone(),
                            show_backside,

                        }
                    }

                    div {
                        class: "flex flex-row w-full h-full overflow-hidden justify-start",
                        style: "max-width: 1200px;",

                        div {
                            class: "w-[600px] p-4 box-border overflow-y-auto overflow-x-hidden",
                            style: "min-height: 0; max-height: 100%;",
                             CardSides {
                                front, back, queue, card: card.clone(), show_backside,
                             }
                        }

                        div {
                            class: "flex-1 box-border relative overflow-y-auto",
                            style: "min-height: 0;",
                            if show_backside.cloned() {
                                MasterySection { history }
                            }
                            RenderDependencies{
                                card: card.clone(),
                                explicit_dependencies,
                                show_backside: show_backside.cloned(),
                                queue: queue.clone(),
                            }
                            RenderDependents{
                                card_id: card.id(),
                                hidden: !(*show_backside.read()),
                            }
                        }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct Queue {
    passed: Vec<CardId>,
    upcoming: Vec<CardId>,
}

impl Queue {
    fn new(cards: NonEmpty<CardId>) -> Self {
        Self {
            passed: vec![],
            upcoming: cards.into(),
        }
    }

    fn next(&mut self) {
        if !self.upcoming.is_empty() {
            self.passed.push(self.upcoming.remove(0));
            if self.upcoming.is_empty() {
                pop_overlay();
            }
        } else {
            pop_overlay();
        }
    }

    pub fn current(&self) -> Option<CardId> {
        self.upcoming.first().cloned()
    }

    fn tot_len(&self) -> usize {
        self.passed_len() + self.upcoming.len()
    }

    fn passed_len(&self) -> usize {
        self.passed.len()
    }
}

#[derive(Clone, Debug)]
pub struct ReviewState {
    pub queue: Signal<Queue>,
    pub tot_len: Memo<usize>,
    pub show_backside: Signal<bool>,
    pub is_done: Memo<bool>,
}

impl ReviewState {
    pub fn new(thecards: NonEmpty<CardId>) -> Self {
        info!("start review for {} cards", thecards.len());

        let queue: Signal<Queue> = Signal::new_in_scope(Queue::new(thecards), ScopeId::APP);

        let is_done: Memo<bool> =
            ScopeId::APP.in_runtime(|| Memo::new(move || queue.read().current().is_none()));

        let tot_len = ScopeId::APP.in_runtime(|| Memo::new(move || queue.read().tot_len()));
        Self {
            tot_len,
            show_backside: Signal::new_in_scope(Default::default(), ScopeId::APP),
            is_done,
            queue,
        }
    }
}

#[component]
fn Infobar(
    card: Arc<Card>,
    tot: Memo<usize>,
    queue: Signal<Queue>,
    show_backside: Signal<bool>,
) -> Element {
    let tot = queue.read().tot_len();
    let pos = queue.read().passed_len();
    let card2 = card.clone();

    rsx! {
        div {
            class: "flex justify-start items-center w-full md:w-auto gap-5",

             div {
        class: "flex justify-start items-center w-full md:w-auto gap-5",

        div {
            class: "flex text-2xl text-gray-700 gap-1",

            button {
                class: "text-2xl text-gray-700",
                onclick: move |_| {
                    let passed: BTreeSet<Input> = queue.read().passed.clone()
                        .into_iter()
                        .map(Input::Card)
                        .collect();
                    let set = SetExpr::Union(passed);
                    let props = CardSelector::new(false, Default::default()).with_set(set).with_edit_collection(false);
                    OverlayEnum::CardSelector(props).append();
                },
                "{pos}"
            }

            span { "/" }

            button {
                class: "text-2xl text-gray-700",
                onclick: move |_| {
                    let total = {
                        let mut passed: BTreeSet<Input> = queue.read().passed.clone()
                            .into_iter()
                            .map(Input::Card)
                            .collect();
                        let upcoming: BTreeSet<Input> = queue.read().upcoming.clone()
                            .into_iter()
                            .map(Input::Card)
                            .collect();
                        passed.extend(upcoming);
                        passed
                    };
                    let set = SetExpr::Union(total);
                    let props = CardSelector::new(false, Default::default()).with_set(set).with_edit_collection(false);
                    OverlayEnum::CardSelector(props).append();
                },
                "{tot}"
            }
        }
    }
            button {
                class: "{crate::styles::READ_BUTTON}",
                onclick: move |_| {
                    let card = card2.clone();
                    OverlayEnum::new_edit_card(card.id()).append();
                },
                "edit"
            }
            Suspend {
                card: card.id(),
                queue,
                show_backside,
            }
        }
    }
}

#[component]
fn Suspend(card: CardId, mut queue: Signal<Queue>, show_backside: Signal<bool>) -> Element {
    let card = Arc::unwrap_or_clone(APP.read().load_card(card));
    let is_suspended = card.is_suspended();
    let txt = if is_suspended { "unsuspend" } else { "suspend" };

    rsx! {
        button {
            class: "{crate::styles::UPDATE_BUTTON}",
            onclick: move |_| {
                let card = card.clone();
                let mut card = card;
                card.set_suspend(!is_suspended);
                queue.write().next();
                show_backside.set(false);
            },
            "{txt}"
        }
    }
}

#[component]
fn RenderDependencies(
    mut card: Arc<Card>,
    explicit_dependencies: Vec<Arc<Card>>,
    show_backside: bool,
    queue: Signal<Queue>,
) -> Element {
    let show_graph = if show_backside {
        "opacity-100 visible"
    } else {
        "opacity-0 invisible"
    };

    let deps = explicit_dependencies.clone();

    let wtf = vec![deps.clone(); deps.len()];
    let my_iter = deps.clone().into_iter().zip(wtf).enumerate();
    let card2 = card.id();

    rsx! {
        div {
            class: "flex flex-col {show_graph} w-full h-auto bg-white p-2 shadow-md rounded-md overflow-y-auto",


            div {
                class: "flex items-center justify-between mb-2",

                AdderHeader {
                    title: "Explicit dependencies",
                    on_add: move |_| {
                        let currcard = card.clone();

                        let fun = MyClosure::new(move |card: CardId| {
                            let  old_card = currcard.clone();
                            let mut old_card = Arc::unwrap_or_clone(old_card);
                            old_card.add_dependency(card).unwrap();
                            let _ = queue.clone().write();
                        });

                        let card = card.clone();
                        let front = format!("{}{}", card.print(), card.display_backside());
                        let props = CardSelector::dependency_picker(fun).with_default_search(front).with_forbidden_cards(vec![card.id()]);
                        OverlayEnum::CardSelector(props).append();

                    },
                 }
            }

            for (idx, (card, deps)) in my_iter{
                div {
                    class: "flex flex-row",
                    button {
                        class: "mb-1 p-1 bg-gray-100 rounded-md text-left",
                        onclick: move|_|{
                            let card = card.clone();
                            let viewer = CardViewer::new_from_card(card);
                            OverlayEnum::CardViewer(viewer).append();
                        },
                        "{card}"
                    }
                    button {
                        class: "p-1 hover:bg-gray-200 hover:border-gray-400 border border-transparent rounded-md transition-colors",
                        onclick: move |_|{
                            let wtf = deps.clone();
                            let removed =  wtf.clone().get(idx).cloned().unwrap();
                            let id = card2;
                            let event = TheLedgerEvent::new_modify(id, CardAction::RemoveDependency(removed.id()));
                            APP.read().inner().provider.cards.modify(event).unwrap();
                            ScopeId::APP.needs_update();
                        },
                        "X"
                    }


                }
           }
        }
    }
}

#[component]
fn RenderEvalText(eval: EvalText) -> Element {
    rsx! {
        div {
            class: "text-lg text-gray-700 text-center",
            p {
                for cmp in eval.components().clone() {
                    match cmp {
                        Either::Left(s) => {
                            rsx! {
                                span { " {s}" }
                            }
                        }
                        Either::Right((s, id)) => {
                            rsx! {
                                button {
                                    class: "inline underline text-blue-600 hover:text-blue-800",
                                    onclick: move |_| {
                                        let card = APP.read().load_card(id);
                                        let props = CardViewer::new_from_card(card);
                                        OverlayEnum::CardViewer(props).append();
                                    },
                                    " {s}"
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
fn CardSides(
    front: EvalText,
    back: EvalText,
    show_backside: Signal<bool>,
    card: Arc<Card>,
    queue: Signal<Queue>,
) -> Element {
    let card = Signal::new_in_scope(Arc::unwrap_or_clone(card), ScopeId::APP);

    let backside_visibility_class = if show_backside() {
        "opacity-100 visible"
    } else {
        "opacity-0 invisible"
    };

    rsx! {
        div {
            class: "flex flex-col items-center w-full",


            div {
                class: "mb-10",
                RenderEvalText { eval: front}
            }

            div {
                class: "flex flex-col w-full items-center {backside_visibility_class}",

                div {
                    class: "w-2/4 h-0.5 bg-gray-300",
                    style: "margin-top: 4px; margin-bottom: 12px;",
                }

                RenderEvalText { eval: back}

            }

            div {
                class: "w-full flex justify-center items-center",
                ReviewButtons{
                    show_backside,
                    card,
                    queue,
                }
            }
        }
    }
}
