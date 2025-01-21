use dioxus::prelude::*;
use std::{rc::Rc, sync::Arc};

use speki_core::{card::CardId, cardfilter::CardFilter, recall_rate::Recall, Card};
use tracing::info;

use crate::{
    overlays::{
        card_selector::{CardSelector, MyClosure},
        cardviewer::CardViewer,
    },
    pages::{Overender, OverlayEnum},
    APP,
};

#[component]
fn RecallButton(
    recall: Recall,
    card: Card,
    mut queue: Signal<Vec<CardId>>,
    mut show_backside: Signal<bool>,
) -> Element {
    let label = match recall {
        Recall::None => "😡",
        Recall::Late => "😠",
        Recall::Some => "🙂",
        Recall::Perfect => "😁",
    };

    rsx! {
        button {
            class: "bg-white mt-6 inline-flex items-center justify-center text-white border-0 py-4 px-6 focus:outline-none hover:bg-gray-700 rounded md:mt-0 text-4xl leading-none",
            onclick: move |_| {
                let card = card.clone();
                spawn(async move{
                    info!("do review");
                    card
                    .clone()
                        .add_review(recall)
                        .await;
                    queue.write().pop();
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
    card: Card,
    queue: Signal<Vec<CardId>>,
) -> Element {
    rsx! {
        div {
            class: "flex flex-col items-center justify-center h-[68px]",

            if !show_backside() {
                button {
                    class: "inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base",
                    onclick: move |_| show_backside.set(true),
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
                            show_backside: show_backside.clone()
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn ReviewRender(
    front: String,
    back: String,
    card: Card,
    queue: Signal<Vec<CardId>>,
    show_backside: Signal<bool>,
    tot: usize,
    overlay: Signal<Option<OverlayEnum>>,
    dependencies: Resource<Vec<(String, Arc<Card>, Signal<Option<OverlayEnum>>)>>,
) -> Element {
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
        queue.clone().write().pop();
        show_backside.clone().set(false);
        spawn(async move {
            card.clone().add_review(recall).await;
        });
    };

    rsx! {
        Overender {
            overlay,
            root: rsx! {
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
                        class: "flex flex-col md:flex-row w-full h-full overflow-hidden",

                        div {
                            class: "flex-1 w-full md:w-1/2 box-border order-1 md:order-2 relative",
                            style: "min-height: 0; flex-grow: 1;",
                            RenderDependencies{
                                card: card.clone(),
                                dependencies,
                                overlay: overlay.clone(),
                                show_backside: show_backside.cloned(),

                            }
                        }

                        div {
                            class: "flex-none w-full md:w-1/2 p-4 box-border overflow-y-auto overflow-x-hidden order-2 md:order-1",
                            style: "min-height: 0; max-height: 100%;",
                             CardSides {
                                front, back, queue, card, show_backside
                             }
                        }
                    }
                }
            }
        },
    }
}

#[derive(Clone, Debug)]
pub struct ReviewState {
    pub queue: Signal<Vec<CardId>>,
    pub card: Resource<Option<Card>>,
    pub tot_len: usize,
    pub front: Resource<String>,
    pub back: Resource<String>,
    pub show_backside: Signal<bool>,
    pub dependencies: Resource<Vec<(String, Arc<Card>, Signal<Option<OverlayEnum>>)>>,
    pub is_done: Memo<bool>,
    pub overlay: Signal<Option<OverlayEnum>>,
}

impl ReviewState {
    pub async fn new_with_filter(cards: Vec<Arc<Card>>, filter: CardFilter) -> Self {
        let mut filtered = vec![];

        for card in cards {
            if filter.filter(card.clone()).await {
                filtered.push(card);
            }
        }

        Self::new(filtered).await
    }

    pub async fn new(cards: Vec<Arc<Card>>) -> Self {
        info!("start review for {} cards", cards.len());

        let mut thecards = vec![];

        for card in cards {
            thecards.push(card.id());
        }

        let overlay: Signal<Option<OverlayEnum>> = Signal::new_in_scope(None, ScopeId::APP);
        let queue: Signal<Vec<CardId>> = Signal::new_in_scope(thecards, ScopeId::APP);

        let is_done: Memo<bool> =
            ScopeId::APP.in_runtime(|| use_memo(move || queue.read().last().is_none()));

        let card = ScopeId::APP.in_runtime(|| {
            use_resource(move || async move {
                match queue.read().last() {
                    Some(id) => {
                        let card = APP.read().load_card(*id).await;
                        Some(Arc::unwrap_or_clone(card))
                    }
                    None => None,
                }
            })
        });

        let dependencies: Resource<Vec<(String, Arc<Card>, Signal<Option<OverlayEnum>>)>> =
            ScopeId::APP.in_runtime(|| {
                use_resource(move || async move {
                    if let Some(Some(card)) = card.cloned() {
                        let mut deps: Vec<(String, Arc<Card>, Signal<Option<OverlayEnum>>)> =
                            vec![];
                        let card = APP.read().load_card(card.id()).await;

                        for dep in &card.dependency_ids().await {
                            let dep = APP.read().load_card(*dep).await;
                            let s = dep.print().await;
                            deps.push((s, dep, overlay.clone()));
                        }

                        deps
                    } else {
                        vec![]
                    }
                })
            });

        let front = ScopeId::APP.in_runtime(|| {
            use_resource(move || async move {
                match card.cloned() {
                    Some(Some(card)) => card.print().await,
                    _ => "".to_string(),
                }
            })
        });
        let back = ScopeId::APP.in_runtime(|| {
            use_resource(move || async move {
                match card.cloned() {
                    Some(Some(card)) => card.display_backside().await.unwrap_or_default(),
                    _ => "".to_string(),
                }
            })
        });

        let tot_len = queue.read().len();
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
    card: Card,
    overlay: Signal<Option<OverlayEnum>>,
    tot: usize,
    queue: Signal<Vec<CardId>>,
) -> Element {
    let pos = tot - queue.read().len();
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
                        let viewer = CardViewer::new_from_card(Arc::new(card), Default::default()).await;
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
fn Suspend(card: Card, mut queue: Signal<Vec<CardId>>) -> Element {
    let is_suspended = card.is_suspended();
    let txt = if is_suspended { "unsuspend" } else { "suspend" };

    rsx! {
        button {
            class: "mt-2 inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base md:mt-0",
            onclick: move |_| {
                let card = card.clone();
                spawn(async move {
                    let mut card = card;
                    card.set_suspend(true).await;
                    queue.write().pop();
                });
            },
            "{txt}"
        }
    }
}

#[component]
fn RenderDependencies(
    card: Card,
    dependencies: Resource<Vec<(String, Arc<Card>, Signal<Option<OverlayEnum>>)>>,
    overlay: Signal<Option<OverlayEnum>>,
    show_backside: bool,
) -> Element {
    let show_graph = if show_backside {
        "opacity-100 visible"
    } else {
        "opacity-0 invisible"
    };

    let deps = dependencies.cloned().unwrap_or_default();

    rsx! {
        div {
            class: "flex flex-col {show_graph} absolute top-0 left-0 w-1/2 h-auto bg-white p-2 shadow-md rounded-md overflow-y-auto",

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

                            let fun = MyClosure(Arc::new(Box::new(move |card: Arc<Card>| {
                                let mut old_card = currcard.clone();
                                spawn(async move {
                                    old_card.add_dependency(card.id()).await;
                                });
                            })));

                            spawn(async move {
                                let props = CardSelector::dependency_picker(fun).await;
                                overlay.clone().set(Some(OverlayEnum::CardSelector(props)));
                            });
                        },
                        "➕"
                    }
                }

            for (name, card, overlay) in deps {
                button {
                    class: "mb-1 p-1 bg-gray-100 rounded-md text-left",
                    onclick: move|_|{
                        let card = card.clone();
                        spawn(async move{
                            let viewer = CardViewer::new_from_card(card, Default::default()).await;
                            overlay.clone().set(Some(OverlayEnum::CardViewer(viewer)));
                        });
                    },
                    "{name}"
                }
            }
        }
    }
}

#[component]
fn CardSides(
    front: String,
    back: String,
    show_backside: Signal<bool>,
    card: Card,
    queue: Signal<Vec<CardId>>,
) -> Element {
    let backside_visibility_class = if show_backside() {
        "opacity-100 visible"
    } else {
        "opacity-0 invisible"
    };

    rsx! {
        div {
            class: "flex flex-col items-center w-full",

            p {
                class: "text-lg text-gray-800 text-center mb-10",
                "{front}"
            }

            div {
                class: "flex flex-col w-full items-center",

                div {
                    class: "w-2/4 h-0.5 bg-gray-300",
                    style: "margin-top: 4px; margin-bottom: 12px;",
                }

                p {
                    class: "text-lg text-gray-700 text-center mb-4 {backside_visibility_class}",
                    "{back}"
                }
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
