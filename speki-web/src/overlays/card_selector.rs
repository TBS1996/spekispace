use std::{future::Future, pin::Pin, sync::Arc};

use dioxus::prelude::*;
use speki_core::{cardfilter::CardFilter, collection::DynCard};
use speki_web::{CardEntry, Node};
use tracing::info;

use crate::{
    components::{CardTy, FilterComp, FilterEditor, GraphRep},
    overlays::cardviewer::CardViewer,
};

use super::{colviewer::CollectionEditor, OverlayEnum};

pub fn overlay_card_viewer(overlay: Signal<Option<OverlayEnum>>) -> MyClosure {
    MyClosure::new(move |card: CardEntry| async move {
        let graph = GraphRep::new().with_hook(overlay_card_viewer(overlay.clone()));
        let viewer = CardViewer::new_from_card(card, graph).await;
        overlay.clone().set(Some(OverlayEnum::CardViewer(viewer)));
    })
}

#[derive(Props, Clone)]
pub struct CardSelector {
    pub title: String,
    pub search: Signal<String>,
    pub on_card_selected: MyClosure,
    pub cards: Resource<Vec<CardEntry>>,
    pub allow_new: bool,
    pub done: Signal<bool>,
    pub dependents: Signal<Vec<Node>>,
    pub allowed_cards: Signal<Vec<CardTy>>,
    pub filtereditor: FilterEditor,
    pub filtermemo: Memo<Option<CardFilter>>,
    pub overlay: Signal<Option<OverlayEnum>>,
    pub collection: CollectionEditor,
    pub col_cards: Resource<Vec<CardEntry>>,
}

impl Default for CardSelector {
    fn default() -> Self {
        Self::new(true, vec![])
    }
}

impl CardSelector {
    pub fn new(with_memo: bool, allowed_cards: Vec<CardTy>) -> Self {
        let allowed_cards = Signal::new_in_scope(allowed_cards, ScopeId::APP);

        let filtereditor = FilterEditor::new_permissive();

        let filtermemo: Memo<Option<CardFilter>> = ScopeId::APP.in_runtime(|| {
            let editor = filtereditor.clone();
            if !with_memo {
                use_memo(|| None)
            } else {
                use_memo(move || {
                    Some(CardFilter {
                        recall: editor.recall.get_value(),
                        rec_recall: editor.rec_recall.get_value(),
                        stability: editor.stability.get_value(),
                        rec_stability: editor.rec_stability.get_value(),
                        finished: editor.finished.get_value(),
                        suspended: editor.suspended.get_value(),
                        pending: editor.pending.get_value(),
                        lapses: editor.lapses.get_value(),
                    })
                })
            }
        });

        let search = Signal::new_in_scope(String::new(), ScopeId::APP);
        let overlay: Signal<Option<OverlayEnum>> =
            Signal::new_in_scope(Default::default(), ScopeId::APP);

        let collection = CollectionEditor::new_unsaved();
        let cards = collection.expanded();

        let allowed = allowed_cards.clone();
        let cards = ScopeId::APP.in_runtime(|| {
            let allowed = allowed.clone();
            let cards = cards.clone();
            use_resource(move || {
                let allowed_cards = allowed.clone();
                async move {
                    let allowed_cards = allowed_cards.clone();
                    let mut filtered_cards: Vec<CardEntry> = vec![];

                    let cards = cards.cloned().unwrap_or_default();

                    for card in cards {
                        if allowed_cards.is_empty()
                            || allowed_cards.read().contains(&CardTy::from_ctype(
                                card.card.read().get_ty().fieldless(),
                            ))
                        {
                            let flag = match filtermemo.cloned() {
                                Some(filter) => filter.filter(Arc::new(card.card.cloned())).await,
                                None => true,
                            };

                            if flag {
                                let front = card.card.read().print().await;

                                if front.contains(&search.cloned().to_lowercase()) {
                                    filtered_cards.push(card);
                                }
                            }
                        }
                    }

                    filtered_cards.sort_by_key(|card| card.card.read().last_modified());
                    filtered_cards.reverse();

                    filtered_cards
                }
            })
        });

        let mut col = collection.clone();
        spawn(async move { col.push_entry(DynCard::Any).await });

        Self {
            title: "select card".to_string(),
            search,
            on_card_selected: overlay_card_viewer(overlay.clone()),
            cards,
            allow_new: false,
            done: Signal::new_in_scope(Default::default(), ScopeId::APP),
            dependents: Signal::new_in_scope(Default::default(), ScopeId::APP),
            allowed_cards,
            filtereditor,
            filtermemo,
            overlay,
            collection,
            col_cards: cards,
        }
    }

    pub fn ref_picker(fun: MyClosure, dependents: Vec<Node>) -> Self {
        Self {
            title: "choose reference".to_string(),
            on_card_selected: fun,
            allow_new: true,
            done: Signal::new_in_scope(false, ScopeId(3)),
            dependents: Signal::new_in_scope(dependents, ScopeId(3)),
            ..Self::new(false, vec![])
        }
    }

    pub fn class_picker(f: MyClosure) -> Self {
        Self::new(false, vec![CardTy::Class])
            .with_title("pick class".into())
            .new_on_card_selected(f)
    }

    pub fn dependency_picker(f: MyClosure) -> Self {
        Self {
            title: "set dependency".to_string(),
            on_card_selected: f,
            allow_new: true,
            ..Self::new(false, vec![])
        }
    }

    pub fn new_on_card_selected(mut self, f: MyClosure) -> Self {
        self.on_card_selected = f;
        self
    }

    pub fn with_dependents(self, deps: Vec<Node>) -> Self {
        self.dependents.clone().write().extend(deps);
        self
    }

    pub fn with_allowed_cards(mut self, deps: Vec<CardTy>) -> Self {
        self.allowed_cards.set(deps);
        self
    }

    pub fn with_title(mut self, title: String) -> Self {
        self.title = title;
        self
    }
}

impl PartialEq for CardSelector {
    fn eq(&self, other: &Self) -> bool {
        self.title == other.title && self.search == other.search
    }
}

#[derive(Clone)]
pub struct MyClosure(
    pub Arc<Box<dyn Fn(CardEntry) -> Pin<Box<dyn Future<Output = ()> + 'static>> + 'static>>,
);

// Example usage:
impl MyClosure {
    pub fn new<F, Fut>(func: F) -> Self
    where
        F: Fn(CardEntry) -> Fut + 'static,
        Fut: Future<Output = ()> + 'static,
    {
        MyClosure(Arc::new(Box::new(move |card| {
            Box::pin(func(card)) as Pin<Box<dyn Future<Output = ()>>>
        })))
    }

    pub async fn call(&self, card: CardEntry) {
        (self.0)(card).await;
    }
}

impl PartialEq for MyClosure {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

#[component]
pub fn CardSelectorRender(
    title: String,
    search: Signal<String>,
    on_card_selected: MyClosure,
    cards: Resource<Vec<CardEntry>>,
    allow_new: bool,
    done: Signal<bool>,
    dependents: Signal<Vec<Node>>,
    allowed_cards: Signal<Vec<CardTy>>,
    filtereditor: FilterEditor,
    filtermemo: Memo<Option<CardFilter>>,
    overlay: Signal<Option<OverlayEnum>>,
    collection: CollectionEditor,
) -> Element {
    info!("render cardselector");
    rsx! {
        div {
            class: "flex flex-row",

        if filtermemo.read().is_some() {
            FilterComp {editor: filtereditor}
        }

        div {
            class: "h-screen flex flex-col w-full max-w-3xl",

            h1 {
                class: "text-lg font-bold mb-4",
                "{title}"
            }

            div {
                if allow_new {
                    NewcardButton { on_card_selected: on_card_selected.clone(), done, overlay, dependents: dependents(), allowed_cards: allowed_cards.cloned(), search }
                }

                input {
                    class: "bg-white w-full max-w-md border border-gray-300 rounded-md p-2 mb-4 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                    value: "{search}",
                    oninput: move |evt| search.set(evt.value().clone()),
                }
            }

            TableRender {
                cards, on_card_selected, done
              }
            }
        }
    }
}

#[component]
fn NewcardButton(
    on_card_selected: MyClosure,
    done: Signal<bool>,
    overlay: Signal<Option<OverlayEnum>>,
    dependents: Vec<Node>,
    allowed_cards: Vec<CardTy>,
    search: String,
) -> Element {
    let closure = Arc::new(on_card_selected.clone());
    rsx! {
        button {
            class: "bg-blue-500 text-white font-medium px-4 py-2 rounded-md hover:bg-blue-600 focus:outline-none focus:ring-2 focus:ring-blue-300 mr-10",
            onclick: move |_| {

                let done = done.clone();
                let closure = closure.clone();
                let hook = MyClosure::new(move |card: CardEntry| {
                    let closure = closure.clone();

                    async move {
                        closure.call(card).await;
                        done.clone().set(true);
                    }
                });

                let viewer = CardViewer::new()
                    .with_hook(hook)
                    .with_dependents(dependents.clone())
                    .with_allowed_cards(allowed_cards.clone())
                    .with_front_text(search.clone());

                viewer.set_graph();

                overlay.clone().set(Some(OverlayEnum::CardViewer(viewer)));

            },
            "new card"
        }
    }
}

#[component]
fn TableRender(
    cards: Resource<Vec<CardEntry>>,
    on_card_selected: MyClosure,
    done: Signal<bool>,
) -> Element {
    let closure = Arc::new(on_card_selected.clone());

    let filtered_cards: Vec<_> = cards
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .take(1000)
        .zip(std::iter::repeat_with(|| Arc::clone(&closure)))
        .map(|(card, closure)| (card.clone(), closure, done.clone()))
        .collect();

    rsx! {
            div {
                class: "flex-1 overflow-y-auto",
                table {
                    class: "min-w-full table-fixed border-collapse border border-gray-200",
                    thead {
                        class: "bg-gray-500",
                        tr {
                            th { class: "border border-gray-300 px-4 py-2 w-2/3", "Front" }
                            th { class: "border border-gray-300 px-4 py-2 w-1/12", "Recall" }
                            th { class: "border border-gray-300 px-4 py-2 w-1/12", "Stability" }
                        }
                    }
                    tbody {
                        for (card, _closure, is_done) in filtered_cards {
                            tr {
                                class: "hover:bg-gray-50 cursor-pointer",
                                onclick: move |_| {
                                    info!("clicky");
                                    let card = card.clone();
                                    let closure = _closure.clone();
                                    let done = is_done.clone();
                                    spawn(async move {
                                        closure.call(card).await;
                                        done.clone().set(true);
                                    });

                                },

                                td { class: "border border-gray-300 px-4 py-2 w-2/3", "{card}" }
                                td { class: "border border-gray-300 px-4 py-2 w-1/12", "{card.card.read().recall_rate().unwrap_or_default():.2}" }
                                td { class: "border border-gray-300 px-4 py-2 w-1/12", "{card.card.read().maybeturity().unwrap_or_default():.1}" }
                            }
                        }
                    }
                }
            }
    }
}
