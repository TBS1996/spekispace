use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
use dioxus::prelude::*;
use either::Either;
use nonempty::NonEmpty;
use std::{collections::BTreeSet, rc::Rc, sync::Arc};

use speki_core::{
    card::{CardId, EvalText},
    cardfilter::CardFilter,
    collection::{DynCard, MaybeCard},
    recall_rate::Recall,
    Card,
};
use tracing::{info, trace};

use crate::{
    components::RenderDependents,
    overlays::{
        card_selector::{CardSelector, MyClosure},
        cardviewer::CardViewer,
    },
    pages::play_audio,
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
    let _label = match recall {
        Recall::None => "😞",
        Recall::Late => "😐",
        Recall::Some => "🙂",
        Recall::Perfect => "😃",
    };

    let label = match recall {
        Recall::None => "1",
        Recall::Late => "2",
        Recall::Some => "3",
        Recall::Perfect => "4",
    };

    rsx! {
        button {
            class: "bg-blue-500 mt-6 inline-flex items-center justify-center text-white border-0 py-4 px-6 focus:outline-none hover:bg-blue-700 rounded text-4xl leading-none",
            onclick: move |_| {
                let mut card = card.clone();
                spawn(async move{
                    info!("do review");
                    card.write()
                        .add_review(recall)
                        .await;
                    queue.write().next();
                    show_backside.set(false);
                });
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

                        if let Some(audio) = card.read().back_audio() {
                            play_audio(&audio.data, "audio/mpeg");
                    }


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

impl ReviewSession {
    pub async fn new(cards: Vec<DynCard>, filter: CardFilter) -> Self {
        let mut selv = Self {
            cards,
            filter,
            thecards: Default::default(),
        };
        let thecards = selv.expand().await;
        selv.thecards = thecards;
        selv
    }

    pub async fn expand(&self) -> BTreeSet<Arc<Card>> {
        let provider = APP.read().inner().card_provider();
        let mut out: BTreeSet<Arc<Card>> = Default::default();

        for card in &self.cards {
            let cards = card.evaluate(provider.clone());
            let cardqty = cards.len();
            for (idx, card) in cards.into_iter().enumerate() {
                if idx % 100 == 0 {
                    trace!("eval {}/{}", idx, cardqty);
                }
                let card = match card {
                    MaybeCard::Id(id) => provider.load(id).unwrap(),
                    MaybeCard::Card(card) => card,
                };

                if self.filter.filter(card.clone()).await {
                    out.insert(card.clone());
                }

                for dependency in card.recursive_dependencies().await {
                    let card = provider.load(dependency).unwrap();
                    if self.filter.filter(card.clone()).await {
                        out.insert(card.clone());
                    }
                }
            }
        }

        out
    }
}

#[component]
pub fn ReviewRender(
    front: Memo<EvalText>,
    back: Memo<EvalText>,
    card: Signal<Card>,
    queue: Signal<Queue>,
    show_backside: Signal<bool>,
    tot: Resource<usize>,
    overlay: Signal<Option<OverlayEnum>>,
    dependencies: Resource<Vec<Signal<Card>>>,
) -> Element {
    let card2 = card.clone();
    let log_event = move |event: Rc<KeyboardData>| {
        let mut card = card2.clone();
        info!("reviewing..");
        let bck = show_backside.cloned();
        let recall = match event.key().to_string().as_str() {
            "1" if bck => Recall::None,
            "2" if bck => Recall::Late,
            "3" if bck => Recall::Some,
            "4" if bck => Recall::Perfect,
            " " => {
                show_backside.clone().set(true);

                if let Some(audio) = card.read().back_audio() {
                    play_audio(&audio.data, "audio/mpeg");
                }

                return;
            }
            _ => return,
        };
        queue.clone().write().next();
        show_backside.clone().set(false);
        spawn(async move {
            card.write().add_review(recall).await;
        });
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
                            overlay: overlay.clone(),
                            tot,
                            queue: queue.clone(),

                        }
                    }

                    div {
                        class: "flex flex-row md:flex-row w-full h-full overflow-hidden",

                        div {
                            class: "flex-1 w-full md:w-1/2 box-border order-1 md:order-2 relative",
                            style: "min-height: 0; flex-grow: 1;",
                            RenderDependencies{
                                card: card.clone(),
                                dependencies,
                                overlay: overlay.clone(),
                                show_backside: show_backside.cloned(),
                                queue: queue.clone(),
                            }
                            RenderDependents{
                                card_id: card.read().id(),
                                overlay: overlay.clone(),
                                hidden: !(*show_backside.read()),
                            }
                        }

                        div {
                            class: "flex-none w-full md:w-1/2 p-4 box-border overflow-y-auto overflow-x-hidden order-2 md:order-1",
                            style: "min-height: 0; max-height: 100%;",
                             CardSides {
                                front, back, queue, card, show_backside, overlay: overlay.clone()
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

    fn set_upcoming(&mut self, cards: Vec<CardId>) {
        self.upcoming = cards;
    }

    fn upcoming(&self) -> Vec<CardId> {
        self.upcoming.clone()
    }

    fn next(&mut self) {
        if !self.upcoming.is_empty() {
            let id = self.current().unwrap();
            spawn(async move {
                if let Some(card) = APP.read().try_load_card(id).await {
                    if let Some(audio) = card.clone().read().front_audio() {
                        play_audio(&audio.data, "audio/mpeg");
                    }
                }
            });

            self.passed.push(self.upcoming.remove(0));
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
    pub card: Resource<Option<Signal<Card>>>,
    pub dependencies: Resource<Vec<Signal<Card>>>,
    pub tot_len: Resource<usize>,
    pub front: Memo<EvalText>,
    pub back: Memo<EvalText>,
    pub show_backside: Signal<bool>,
    pub is_done: Memo<bool>,
    pub overlay: Signal<Option<OverlayEnum>>,
}

impl ReviewState {
    pub async fn new_with_filter(cards: Vec<DynCard>, filter: CardFilter) -> Self {
        let session = ReviewSession::new(cards, filter).await;
        Self::new(session.thecards.into_iter().map(|c| c.id()).collect())
    }

    pub fn new(thecards: Vec<CardId>) -> Self {
        info!("start review for {} cards", thecards.len());

        let overlay: Signal<Option<OverlayEnum>> = Signal::new_in_scope(None, ScopeId::APP);
        let queue: Signal<Queue> = Signal::new_in_scope(Queue::new(thecards), ScopeId::APP);

        let is_done: Memo<bool> =
            ScopeId::APP.in_runtime(|| use_memo(move || queue.read().current().is_none()));

        let card = ScopeId::APP.in_runtime(|| {
            use_resource(move || async move {
                match queue.read().current() {
                    Some(id) => APP.read().try_load_card(id).await,
                    None => None,
                }
            })
        });

        let dependencies: Resource<Vec<Signal<Card>>> = ScopeId::APP.in_runtime(|| {
            use_resource(move || async move {
                if let Some(Some(card)) = card.cloned() {
                    let mut deps: Vec<Signal<Card>> = vec![];

                    for dep in &card.read().dependencies() {
                        if let Some(dep) = APP.read().try_load_card(*dep).await {
                            deps.push(dep);
                        }
                    }
                    deps
                } else {
                    vec![]
                }
            })
        });

        let front = ScopeId::APP.in_runtime(|| {
            use_memo(move || {
                info!("updating front card resource in review!");
                match card.cloned() {
                    Some(Some(card)) => card.read().front_side().clone(),
                    _ => EvalText::default(),
                }
            })
        });

        let back = ScopeId::APP.in_runtime(|| {
            use_memo(move || match card.cloned() {
                Some(Some(card)) => card.read().backside().clone(),
                _ => EvalText::default(),
            })
        });

        let tot_len =
            ScopeId::APP.in_runtime(|| use_resource(move || async move { queue.read().tot_len() }));
        Self {
            card,
            tot_len,
            front,
            back,
            show_backside: Signal::new_in_scope(Default::default(), ScopeId::APP),
            dependencies,
            is_done,
            queue,
            overlay,
        }
    }
}

#[component]
fn Infobar(
    card: Signal<Card>,
    overlay: Signal<Option<OverlayEnum>>,
    tot: Resource<usize>,
    queue: Signal<Queue>,
) -> Element {
    let tot = queue.read().tot_len();
    let pos = queue.read().passed_len();
    let card2 = card.clone();

    rsx! {
        div {
            class: "flex justify-start items-center w-full md:w-auto gap-5",
            h2 {
                class: "text-2xl text-gray-700",
                "{pos}/{tot}"
            }


            button {
                class: "cursor-pointer text-gray-500 hover:text-gray-700",
                onclick: move |_| {
                    let card = card2.clone();
                    let overlay = overlay.clone();
                    spawn(async move {
                        let card = card.clone();
                        let viewer = CardViewer::new_from_card(card, Default::default()).await;
                        let viewer = OverlayEnum::CardViewer(viewer);
                        overlay.clone().set(Some(viewer));
                    });
                },
                "✏️"
            }
            Suspend {
                card,
                queue,
            }
        }
    }
}

#[component]
fn Suspend(card: Signal<Card>, mut queue: Signal<Queue>) -> Element {
    let is_suspended = card.read().is_suspended();
    let txt = if is_suspended { "unsuspend" } else { "suspend" };

    rsx! {
        button {
            class: "mt-2 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base md:mt-0",
            onclick: move |_| {
                let card = card.clone();
                spawn(async move {
                    let mut card = card;
                    card.write().set_suspend(!is_suspended).await;
                    queue.write().next();
                });
            },
            "{txt}"
        }
    }
}

#[component]
fn RenderDependencies(
    card: Signal<Card>,
    dependencies: Resource<Vec<Signal<Card>>>,
    overlay: Signal<Option<OverlayEnum>>,
    show_backside: bool,
    queue: Signal<Queue>,
) -> Element {
    let show_graph = if show_backside {
        "opacity-100 visible"
    } else {
        "opacity-0 invisible"
    };

    let deps = dependencies.cloned().unwrap_or_default();

    rsx! {
        div {
            class: "flex flex-col {show_graph} w-full h-auto bg-white p-2 shadow-md rounded-md overflow-y-auto",


            div {
                class: "flex items-center justify-between mb-2",

                h4 {
                    class: "font-bold",
                    "Dependencies"
                }

                    button {
                        class: "p-1 hover:bg-gray-200 hover:border-gray-400 border border-transparent rounded-md transition-colors",
                        onclick: move |_| {
                            let currcard = card.clone();

                            let fun = MyClosure::new(move |card: Signal<Card>| {
                                let mut old_card = currcard.clone();
                                async move {
                                    old_card.write().add_dependency(card.read().id()).await;
                                    let _ = queue.write();
                                }
                            });

                            spawn(async move {
                                let front = format!("{}{}", card.read().print(), card.read().display_backside());
                                let props = CardSelector::dependency_picker(fun).with_default_search(front).with_forbidden_cards(vec![card.read().id()]);
                                overlay.clone().set(Some(OverlayEnum::CardSelector(props)));
                            });
                        },
                        "➕"
                    }
                }

            for card in deps {
                button {
                    class: "mb-1 p-1 bg-gray-100 rounded-md text-left",
                    onclick: move|_|{
                        let card = card.clone();
                        spawn(async move{
                            let viewer = CardViewer::new_from_card(card, Default::default()).await;
                            overlay.clone().set(Some(OverlayEnum::CardViewer(viewer)));
                        });
                    },
                    "{card}"
                }
            }
        }
    }
}

#[component]
fn RenderEvalText(eval: Memo<EvalText>, overlay: Signal<Option<OverlayEnum>>) -> Element {
    rsx! {
        div {
            class: "text-lg text-gray-700 text-center",
            p {
                for cmp in eval.read().components().clone() {
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
                                        spawn(async move {
                                            let card = APP.read().load_card_sync(id);
                                            let props = CardViewer::new_from_card(card, Default::default()).await;
                                            overlay.clone().set(Some(OverlayEnum::CardViewer(props)));
                                        });
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
    front: Memo<EvalText>,
    back: Memo<EvalText>,
    show_backside: Signal<bool>,
    card: Signal<Card>,
    queue: Signal<Queue>,
    overlay: Signal<Option<OverlayEnum>>,
) -> Element {
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
                RenderEvalText { eval: front, overlay: overlay.clone() }
            }




            div {
                class: "flex flex-col w-full items-center {backside_visibility_class}",

                div {
                    class: "w-2/4 h-0.5 bg-gray-300",
                    style: "margin-top: 4px; margin-bottom: 12px;",
                }

                RenderEvalText { eval: back, overlay: overlay.clone() }

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
