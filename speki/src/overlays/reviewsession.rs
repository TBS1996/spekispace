use dioxus::prelude::*;
use either::Either;
use ledgerstore::TheLedgerEvent;
use std::{collections::BTreeSet, rc::Rc, sync::Arc};

use speki_core::{
    card::{CardId, EvalText},
    cardfilter::CardFilter,
    collection::DynCard,
    ledger::CardAction,
    recall_rate::Recall,
    Card,
};
use tracing::info;

use crate::{
    append_overlay,
    components::RenderDependents,
    overlays::{
        card_selector::{CardSelector, MyClosure},
        cardviewer::CardViewer,
    },
    pop_overlay, APP,
};

use super::OverlayEnum;

#[component]
fn RecallButton(
    recall: Recall,
    card: Signal<Card>,
    mut queue: Signal<Queue>,
    mut show_backside: Signal<bool>,
) -> Element {
    let label = match recall {
        Recall::None => "😞",
        Recall::Late => "😐",
        Recall::Some => "🙂",
        Recall::Perfect => "😃",
    };

    rsx! {
        button {
            class: "bg-blue-500 mt-6 inline-flex items-center justify-center text-white border-0 py-4 px-6 focus:outline-none hover:bg-blue-700 rounded text-4xl leading-none font-emoji",
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
                    class: "inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base",
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
    show_backside: Signal<bool>,
    tot: Memo<usize>,
) -> Element {
    let card_id = match queue.read().current() {
        Some(id) => id,
        None => {
            return rsx! {"if you can read this, i messed up"};
        }
    };

    let card = match APP.read().inner().card_provider.load(card_id) {
        Some(card) => card,
        None => {
            queue.write().next();
            return rsx! {};
        }
    };

    let explicit_dependencies: Vec<Signal<Card>> = {
        let mut deps: Vec<Signal<Card>> = vec![];

        for dep in &card.explicit_dependencies() {
            let dep = APP.read().load_card(*dep);
            deps.push(dep);
        }
        deps
    };

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
                        class: "flex flex-row md:flex-row w-full h-full overflow-hidden",

                        div {
                            class: "flex-1 w-full md:w-1/2 box-border order-1 md:order-2 relative",
                            style: "min-height: 0; flex-grow: 1;",
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

                        div {
                            class: "flex-none w-full md:w-1/2 p-4 box-border overflow-y-auto overflow-x-hidden order-2 md:order-1",
                            style: "min-height: 0; max-height: 100%;",
                             CardSides {
                                front, back, queue, card, show_backside,
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
    fn new(cards: Vec<CardId>) -> Self {
        Self {
            passed: vec![],
            upcoming: cards,
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

    fn current(&self) -> Option<CardId> {
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
    pub fn new(thecards: Vec<CardId>) -> Self {
        info!("start review for {} cards", thecards.len());

        let queue: Signal<Queue> = Signal::new_in_scope(Queue::new(thecards), ScopeId::APP);

        let is_done: Memo<bool> =
            ScopeId::APP.in_runtime(|| use_memo(move || queue.read().current().is_none()));

        let tot_len = ScopeId::APP.in_runtime(|| use_memo(move || queue.read().tot_len()));
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
    let card = Signal::new_in_scope(Arc::unwrap_or_clone(card), ScopeId::APP);
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
                    let passed: Vec<DynCard> = queue.read().passed.clone()
                        .into_iter()
                        .map(DynCard::Card)
                        .collect();
                    let props = CardSelector::new(false, Default::default()).with_dyncards(passed).with_edit_collection(false);
                    append_overlay(OverlayEnum::CardSelector(props));
                },
                "{pos}"
            }

            span { "/" }

            button {
                class: "text-2xl text-gray-700",
                onclick: move |_| {
                    let total = {
                        let mut passed: Vec<DynCard> = queue.read().passed.clone()
                            .into_iter()
                            .map(DynCard::Card)
                            .collect();
                        let upcoming: Vec<DynCard> = queue.read().upcoming.clone()
                            .into_iter()
                            .map(DynCard::Card)
                            .collect();
                        passed.extend(upcoming);
                        passed
                    };
                    let props = CardSelector::new(false, Default::default()).with_dyncards(total).with_edit_collection(false);
                    append_overlay(OverlayEnum::CardSelector(props));
                },
                "{tot}"
            }
        }
    }
            button {
                class: "{crate::styles::BLACK_BUTTON}",
                onclick: move |_| {
                    let card = card2.clone();
                    let card = card.clone();
                    let viewer = CardViewer::new_from_card(card);
                    let viewer = OverlayEnum::CardViewer(viewer);
                    append_overlay(viewer);
                },
                "edit"
            }
            Suspend {
                card,
                queue,
                show_backside,
            }
        }
    }
}

#[component]
fn Suspend(card: Signal<Card>, mut queue: Signal<Queue>, show_backside: Signal<bool>) -> Element {
    let is_suspended = card.read().is_suspended();
    let txt = if is_suspended { "unsuspend" } else { "suspend" };

    rsx! {
        button {
            class: "mt-2 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base md:mt-0",
            onclick: move |_| {
                let card = card.clone();
                let mut card = card;
                card.write().set_suspend(!is_suspended);
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
    explicit_dependencies: Vec<Signal<Card>>,
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

    rsx! {
        div {
            class: "flex flex-col {show_graph} w-full h-auto bg-white p-2 shadow-md rounded-md overflow-y-auto",


            div {
                class: "flex items-center justify-between mb-2",

                h4 {
                    class: "font-bold",
                    "Explicit dependencies"
                }

                    button {
                        class: "p-1 hover:bg-gray-200 hover:border-gray-400 border border-transparent rounded-md transition-colors",
                        onclick: move |_| {
                            let currcard = card.clone();

                            let fun = MyClosure::new(move |card: Signal<Card>| {
                                let  old_card = currcard.clone();
                                let mut old_card = Arc::unwrap_or_clone(old_card);
                                old_card.add_dependency(card.read().id()).unwrap();
                                let _ = queue.clone().write();
                            });

                            let card = card.clone();
                            let front = format!("{}{}", card.print(), card.display_backside());
                            let props = CardSelector::dependency_picker(fun).with_default_search(front).with_forbidden_cards(vec![card.id()]);
                            append_overlay(OverlayEnum::CardSelector(props));
                        },
                        "➕"
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
                            append_overlay(OverlayEnum::CardViewer(viewer));
                        },
                        "{card}"
                    }
                    button {
                        class: "p-1 hover:bg-gray-200 hover:border-gray-400 border border-transparent rounded-md transition-colors",
                        onclick: move |_|{
                            let wtf = deps.clone();
                            let mut thecard = card.clone();
                            let removed =  wtf.clone().get(idx).cloned().unwrap();
                            let id = card.read().id();
                            let event = TheLedgerEvent::new_modify(id, CardAction::RemoveDependency(removed.read().id()));
                            APP.read().inner().provider.cards.modify(event).unwrap();

                            let new_card = Arc::unwrap_or_clone(APP.read().inner().card_provider.load(id).unwrap());
                            thecard.set(new_card);
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
                                        append_overlay(OverlayEnum::CardViewer(props));
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
