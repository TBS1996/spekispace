use std::sync::Arc;

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
    MyClosure(Arc::new(Box::new(move |card: Arc<Card>| {
        spawn(async move {
            let graph = GraphRep::new().with_hook(overlay_card_viewer(overlay.clone()));
            let viewer = CardViewer::new_from_card(card, graph).await;
            overlay.clone().set(Some(OverlayEnum::CardViewer(viewer)));
        });
    })))
}

#[derive(Props, Clone)]
pub struct CardSelector {
    pub title: String,
    pub search: Signal<String>,
    pub on_card_selected: MyClosure,
    pub all_cards: Signal<Vec<CardEntry>>,
    pub filtered_cards: Signal<Vec<CardEntry>>,
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
        Self::new(true)
    }
}

impl CardSelector {
    pub fn new(with_memo: bool) -> Self {
        let filtereditor = FilterEditor::new_permissive();
        let filtermemo = if with_memo {
            Some(filtereditor.memo())
        } else {
            None
        };
        let search = Signal::new_in_scope(String::new(), ScopeId::APP);
        let overlay: Signal<Option<OverlayEnum>> =
            Signal::new_in_scope(Default::default(), ScopeId::APP);

        Self {
            title: "select card".to_string(),
            search,
            on_card_selected: overlay_card_viewer(overlay.clone()),
            all_cards: Signal::new_in_scope(Default::default(), ScopeId::APP),
            filtered_cards: Signal::new_in_scope(Default::default(), ScopeId::APP),
            allow_new: false,
            done: Signal::new_in_scope(Default::default(), ScopeId::APP),
            filter: None,
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
        let selv = Self {
            title: "choose reference".to_string(),
            on_card_selected: fun,
            allow_new: true,
            done: Signal::new_in_scope(false, ScopeId(3)),
            filter,
            dependents: Signal::new_in_scope(dependents, ScopeId(3)),
            ..Self::new(false)
        };

        let selv2 = selv.clone();

        selv2.init_lol().await;

        selv
    }

    pub async fn class_picker(f: MyClosure) -> Self {
        let filter: Callback<CardType, bool> =
            ScopeId::APP.in_runtime(|| Callback::new(move |ty: CardType| ty.is_class()));

        let mut selv = Self::new(false)
            .with_title("pick class".into())
            .new_on_card_selected(f);

        selv.filter = Some(filter);

        let selv2 = selv.clone();

        selv2.init_lol().await;

        selv
    }

    pub async fn dependency_picker(f: MyClosure) -> Self {
        let mut selv = Self::new(false)
            .with_title("set dependency".into())
            .new_on_card_selected(f);
        selv.allow_new = true;

        let selv2 = selv.clone();
        selv2.init_lol().await;
        info!("after init!");

        selv
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

    pub async fn init_lol(&self) {
        info!("render hook in cardselector :)");
        let sig = self.all_cards.clone();
        let selv = self.clone();
        let cards = APP.cloned().load_all(None).await;
        let mut entries = vec![];

        for card in cards {
            if selv
                .filter
                .clone()
                .map(|filter| (filter)(card.get_ty()))
                .unwrap_or(true)
            {
                entries.push(CardEntry::new(card).await);
            }
        }

        sig.clone().set(entries);
    }
}

impl PartialEq for CardSelector {
    fn eq(&self, other: &Self) -> bool {
        self.title == other.title && self.search == other.search
    }
}

#[derive(Clone)]
pub struct MyClosure(pub Arc<Box<dyn Fn(Arc<Card>)>>);

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
    all_cards: Signal<Vec<CardEntry>>,
    filtered_cards: Signal<Vec<CardEntry>>,
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
    let title = &title;

    let filtered_cards = filtered_cards.clone();
    let search = search.clone();
    let cardfilter = filtermemo.clone();
    let all_cards = all_cards.clone();
    use_effect(move || {
        info!("recompute cards");
        let filtered_cards = filtered_cards.clone();
        let search = search.cloned();
        let filter = cardfilter.map(|mem| mem.cloned());

        spawn(async move {
            let mut filtered = vec![];
            for card in all_cards() {
                if let Some(filter) = filter.clone() {
                    if filter.filter(card.card.clone()).await {
                        if card.front.to_lowercase().contains(&search.to_lowercase()) {
                            filtered.push(card);
                        }
                    }
                } else {
                    if card.front.to_lowercase().contains(&search.to_lowercase()) {
                        filtered.push(card);
                    }
                }
            }

            filtered_cards.clone().set(filtered);
        });
    });

    let mut search = search.clone();

    let closure = Arc::new(on_card_selected.clone());

    let filtered_cards: Vec<_> = filtered_cards
        .cloned()
        .into_iter()
        .take(1000)
        .zip(std::iter::repeat_with(|| Arc::clone(&closure)))
        .map(|(card, closure)| (card.clone(), closure, done.clone()))
        .collect();

    use_hook(move || {
        info!("rendering uhhh hook in cardselector :)");
        let sig = all_cards.clone();
        let filter = filter.clone();
        spawn(async move {
            let cards = APP.cloned().load_all(None).await;
            let mut entries = vec![];

            for card in cards {
                if filter
                    .clone()
                    .map(|filter| (filter)(card.get_ty()))
                    .unwrap_or(true)
                {
                    entries.push(CardEntry::new(card).await);
                }
            }

            sig.clone().set(entries);
        });
    });

    let filteditor = filtereditor.clone();
    let filter_render = filtermemo.is_some();

    rsx! {
        div {
            class: "flex flex-row",

        if filter_render {
            FilterComp {editor: filteditor}
        }

        div {
            class: "h-screen flex flex-col w-full max-w-3xl",

            h1 {
                class: "text-lg font-bold mb-4",
                "{title}"
            }

            div {

                if allow_new {
                    button {
                        class: "bg-blue-500 text-white font-medium px-4 py-2 rounded-md hover:bg-blue-600 focus:outline-none focus:ring-2 focus:ring-blue-300 mr-10",
                        onclick: move |_| {

                            let done = done.clone();
                            let closure = closure.clone();
                            let hook = move |card: Arc<Card>| {
                                (closure.0)(card);
                                done.clone().set(true);
                            };

                            let mut viewer = CardViewer::new()
                                .with_title("create new card".to_string())
                                .with_hook(Arc::new(Box::new(hook)))
                                .with_dependents(dependents.cloned())
                                .with_allowed_cards(allowed_cards.clone())
                                .with_front_text(search.cloned());

                            if let Some(filter) = filter.clone() {
                                viewer = viewer.with_filter(filter);
                            }

                            viewer.set_graph();

                            overlay.clone().set(Some(OverlayEnum::CardViewer(viewer)));

                        },
                        "new card"
                     }
                }


                input {
                    class: "bg-white w-full max-w-md border border-gray-300 rounded-md p-2 mb-4 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                    value: "{search}",
                    oninput: move |evt| search.set(evt.value().clone()),
                }
            }


            div {
                class: "flex-1 overflow-y-auto", // Scrollable container, takes up remaining space
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
                                    info!("omggggggggggggggggggggggggg");
                                    let card = card.clone();
                                    let closure = _closure.clone();
                                    let done = is_done.clone();
                                    (closure.0)(card.card.clone());
                                    done.clone().set(true);

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
    }
}
