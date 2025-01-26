use std::{future::Future, pin::Pin, sync::Arc};

use dioxus::prelude::*;
use speki_core::{cardfilter::CardFilter, Card, CardType};
use speki_web::Node;
use tracing::info;

use crate::{
    components::{CardTy, FilterComp, FilterEditor, GraphRep},
    overlays::cardviewer::CardViewer,
    pages::CardEntry,
    APP,
};

use super::OverlayEnum;

pub fn overlay_card_viewer(overlay: Signal<Option<OverlayEnum>>) -> MyClosure {
    MyClosure::new(move |card: Arc<Card>| async move {
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
    pub filter: Option<Callback<CardType, bool>>,
    pub dependents: Signal<Vec<Node>>,
    pub allowed_cards: Vec<CardTy>,
    pub filtereditor: FilterEditor,
    pub filtermemo: Option<Memo<CardFilter>>,
    pub overlay: Signal<Option<OverlayEnum>>,
}

impl Default for CardSelector {
    fn default() -> Self {
        Self::new(true, None)
    }
}

impl CardSelector {
    pub fn new(with_memo: bool, filter: Option<Callback<CardType, bool>>) -> Self {
        let filtereditor = FilterEditor::new_permissive();
        let filtermemo = if with_memo {
            Some(filtereditor.memo())
        } else {
            None
        };
        let search = Signal::new_in_scope(String::new(), ScopeId::APP);
        let overlay: Signal<Option<OverlayEnum>> =
            Signal::new_in_scope(Default::default(), ScopeId::APP);

        let cards = ScopeId::APP.in_runtime(|| {
            use_resource(move || async move {
                let mut filtered_cards: Vec<CardEntry> = vec![];

                let cards = APP.cloned().load_all(None).await;

                for card in cards {
                    if filter
                        .as_ref()
                        .map(|filter| (filter)(card.get_ty()))
                        .unwrap_or(true)
                    {
                        let flag = match filtermemo.clone() {
                            Some(filter) => filter.cloned().filter(card.clone()).await,
                            None => true,
                        };

                        if flag {
                            if card
                                .print()
                                .await
                                .to_lowercase()
                                .contains(&search.cloned().to_lowercase())
                            {
                                filtered_cards.push(CardEntry::new(card).await);
                            }
                        }
                    }
                }

                filtered_cards.sort_by_key(|card| card.card.last_modified());
                filtered_cards.reverse();

                filtered_cards
            })
        });

        Self {
            title: "select card".to_string(),
            search,
            on_card_selected: overlay_card_viewer(overlay.clone()),
            cards,
            allow_new: false,
            done: Signal::new_in_scope(Default::default(), ScopeId::APP),
            filter,
            dependents: Signal::new_in_scope(Default::default(), ScopeId::APP),
            allowed_cards: vec![],
            filtereditor,
            filtermemo,
            overlay,
        }
    }

    pub async fn ref_picker(
        fun: MyClosure,
        dependents: Vec<Node>,
        filter: Option<Callback<CardType, bool>>,
    ) -> Self {
        Self {
            title: "choose reference".to_string(),
            on_card_selected: fun,
            allow_new: true,
            done: Signal::new_in_scope(false, ScopeId(3)),
            dependents: Signal::new_in_scope(dependents, ScopeId(3)),
            ..Self::new(false, filter)
        }
    }

    pub async fn class_picker(f: MyClosure) -> Self {
        let filter: Callback<CardType, bool> =
            ScopeId::APP.in_runtime(|| Callback::new(move |ty: CardType| ty.is_class()));

        Self::new(false, Some(filter))
            .with_title("pick class".into())
            .new_on_card_selected(f)
    }

    pub async fn dependency_picker(f: MyClosure) -> Self {
        Self {
            title: "set dependency".to_string(),
            on_card_selected: f,
            allow_new: true,
            ..Self::new(false, None)
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
        self.allowed_cards = deps;
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
    pub Arc<Box<dyn Fn(Arc<Card>) -> Pin<Box<dyn Future<Output = ()> + 'static>> + 'static>>,
);

// Example usage:
impl MyClosure {
    pub fn new<F, Fut>(func: F) -> Self
    where
        F: Fn(Arc<Card>) -> Fut + 'static,
        Fut: Future<Output = ()> + 'static,
    {
        MyClosure(Arc::new(Box::new(move |card| {
            Box::pin(func(card)) as Pin<Box<dyn Future<Output = ()>>>
        })))
    }

    pub async fn call(&self, card: Arc<Card>) {
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
    filter: Option<Callback<CardType, bool>>,
    dependents: Signal<Vec<Node>>,
    allowed_cards: Vec<CardTy>,
    filtereditor: FilterEditor,
    filtermemo: Option<Memo<CardFilter>>,
    overlay: Signal<Option<OverlayEnum>>,
) -> Element {
    info!("render cardselector");
    rsx! {
        div {
            class: "flex flex-row",

        if filtermemo.is_some() {
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
                    NewcardButton { on_card_selected: on_card_selected.clone(), done, overlay, dependents: dependents(), allowed_cards, search, filter }
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
    filter: Option<Callback<CardType, bool>>,
) -> Element {
    let closure = Arc::new(on_card_selected.clone());
    rsx! {
        button {
            class: "bg-blue-500 text-white font-medium px-4 py-2 rounded-md hover:bg-blue-600 focus:outline-none focus:ring-2 focus:ring-blue-300 mr-10",
            onclick: move |_| {

                let done = done.clone();
                let closure = closure.clone();
                let hook = MyClosure::new(move |card: Arc<Card>| {
                    let closure = closure.clone();

                    async move {
                        closure.call(card).await;
                        done.clone().set(true);
                    }
                });

                let mut viewer = CardViewer::new()
                    .with_title("create new card".to_string())
                    .with_hook(hook)
                    .with_dependents(dependents.clone())
                    .with_allowed_cards(allowed_cards.clone())
                    .with_front_text(search.clone());

                if let Some(filter) = filter.clone() {
                    viewer = viewer.with_filter(filter);
                }

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
                                        closure.call(card.card).await;
                                        done.clone().set(true);
                                    });

                                },

                                td { class: "border border-gray-300 px-4 py-2 w-2/3", "{card.front}" }
                                td { class: "border border-gray-300 px-4 py-2 w-1/12", "{card.card.recall_rate().unwrap_or_default():.2}" }
                                td { class: "border border-gray-300 px-4 py-2 w-1/12", "{card.card.maybeturity().unwrap_or_default():.1}" }
                            }
                        }
                    }
                }
            }
    }
}
