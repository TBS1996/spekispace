use std::sync::Arc;

use dioxus::prelude::*;
use speki_core::{cardfilter::CardFilter, Card, CardType};
use speki_web::Node;
use tracing::info;

use crate::{
    components::{CardTy, FilterEditor, GraphRep, Komponent},
    overlays::{cardviewer::CardViewer, Overlay},
    pages::CardEntry,
    APP, OVERLAY,
};

pub fn overlay_card_viewer() -> Arc<Box<dyn Fn(Arc<Card>)>> {
    Arc::new(Box::new(move |card: Arc<Card>| {
        spawn(async move {
            let graph = GraphRep::new().with_hook(overlay_card_viewer());
            let viewer = CardViewer::new_from_card(card, graph).await;
            OVERLAY.cloned().replace(Box::new(viewer));
        });
    }))
}

#[derive(Props, Clone)]
pub struct CardSelector {
    title: String,
    search: Signal<String>,
    on_card_selected: Arc<Box<dyn Fn(Arc<Card>)>>,
    all_cards: Signal<Vec<CardEntry>>,
    filtered_cards: Signal<Vec<CardEntry>>,
    allow_new: bool,
    done: Signal<bool>,
    filter: Option<Arc<Box<dyn Fn(CardType) -> bool>>>,
    dependents: Signal<Vec<Node>>,
    allowed_cards: Vec<CardTy>,
    filtereditor: FilterEditor,
    filtermemo: Memo<CardFilter>,
}

impl Default for CardSelector {
    fn default() -> Self {
        Self::new()
    }
}

impl CardSelector {
    pub fn new() -> Self {
        let filtereditor = FilterEditor::new_permissive();
        let filtermemo = filtereditor.memo();
        let search = Signal::new_in_scope(String::new(), ScopeId::APP);

        Self {
            title: "select card".to_string(),
            search,
            on_card_selected: overlay_card_viewer(),
            all_cards: Signal::new_in_scope(Default::default(), ScopeId::APP),
            filtered_cards: Signal::new_in_scope(Default::default(), ScopeId::APP),
            allow_new: false,
            done: Signal::new_in_scope(Default::default(), ScopeId::APP),
            filter: None,
            dependents: Signal::new_in_scope(Default::default(), ScopeId::APP),
            allowed_cards: vec![],
            filtereditor,
            filtermemo,
        }
    }

    pub async fn ref_picker(
        fun: Arc<Box<dyn Fn(Arc<Card>)>>,
        dependents: Vec<Node>,
        filter: Option<Arc<Box<dyn Fn(CardType) -> bool>>>,
    ) -> Self {
        let selv = Self {
            title: "choose reference".to_string(),
            on_card_selected: fun,
            allow_new: true,
            done: Signal::new_in_scope(false, ScopeId(3)),
            filter,
            dependents: Signal::new_in_scope(dependents, ScopeId(3)),
            ..Default::default()
        };

        let selv2 = selv.clone();

        selv2.init_lol().await;

        selv
    }

    pub async fn class_picker(f: Box<dyn Fn(Arc<Card>)>) -> Self {
        let filter: Box<dyn Fn(CardType) -> bool> = Box::new(move |ty: CardType| ty.is_class());

        let selv = Self {
            title: "pick class".to_string(),
            on_card_selected: Arc::new(f),
            allow_new: true,
            done: Signal::new_in_scope(false, ScopeId(3)),
            filter: Some(Arc::new(filter)),
            ..Default::default()
        };

        let selv2 = selv.clone();

        selv2.init_lol().await;

        selv
    }

    pub async fn dependency_picker(f: Box<dyn Fn(Arc<Card>)>) -> Self {
        let mut selv = Self::new()
            .with_title("set dependency".into())
            .new_on_card_selected(f);
        selv.allow_new = true;

        let selv2 = selv.clone();
        selv2.init_lol().await;
        info!("after init!");

        selv
    }

    pub fn new_on_card_selected(mut self, f: Box<dyn Fn(Arc<Card>)>) -> Self {
        self.on_card_selected = Arc::new(f);
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

impl Overlay for CardSelector {
    fn is_done(&self) -> Signal<bool> {
        self.done.clone()
    }
}

impl PartialEq for CardSelector {
    fn eq(&self, other: &Self) -> bool {
        self.title == other.title && self.search == other.search
    }
}

impl Komponent for CardSelector {
    /// Selects a card from the collection and calls a closure on it.
    fn render(&self) -> Element {
        info!("render cardselector");
        let title = &self.title;

        let filtered_cards = self.filtered_cards.clone();
        let search = self.search.clone();
        let cardfilter = self.filtermemo.clone();
        let all_cards = self.all_cards.clone();
        use_effect(move || {
            info!("recompute cards");
            let filtered_cards = filtered_cards.clone();
            let search = search.cloned();
            let filter = cardfilter.cloned();

            spawn(async move {
                let mut filtered = vec![];
                for card in all_cards() {
                    if filter.filter(card.card.clone()).await {
                        if card.front.to_lowercase().contains(&search.to_lowercase()) {
                            filtered.push(card);
                        }
                    }
                }

                filtered_cards.clone().set(filtered);
            });
        });

        let mut search = self.search.clone();

        let closure = Arc::new(self.on_card_selected.clone());

        let filtered_cards: Vec<_> = self
            .filtered_cards
            .cloned()
            .into_iter()
            .take(1000)
            .zip(std::iter::repeat_with(|| Arc::clone(&closure)))
            .map(|(card, closure)| (card.clone(), closure, self.done.clone()))
            .collect();

        use_hook(move || {
            info!("rendering uhhh hook in cardselector :)");
            let sig = self.all_cards.clone();
            let selv = self.clone();
            spawn(async move {
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
            });
        });

        let selv = self.clone();

        let filteditor = self.filtereditor.clone();

        rsx! {
            div {
                class: "flex flex-row",

            { filteditor.render() }

            div {
                class: "h-screen flex flex-col w-full max-w-3xl",

                h1 {
                    class: "text-lg font-bold mb-4",
                    "{title}"
                }

                div {

                    if self.allow_new {
                        button {
                            class: "bg-blue-500 text-white font-medium px-4 py-2 rounded-md hover:bg-blue-600 focus:outline-none focus:ring-2 focus:ring-blue-300 mr-10",
                            onclick: move |_| {

                                let done = selv.is_done().clone();
                                let closure = closure.clone();
                                let hook = move |card: Arc<Card>| {
                                    done.clone().set(true);
                                    (closure)(card);
                                };

                                let mut viewer = CardViewer::new()
                                    .with_title("create new card".to_string())
                                    .with_hook(Arc::new(Box::new(hook)))
                                    .with_dependents(selv.dependents.cloned())
                                    .with_allowed_cards(selv.allowed_cards.clone())
                                    .with_front_text(selv.search.cloned());

                                if let Some(filter) = selv.filter.clone() {
                                    viewer = viewer.with_filter(filter);
                                }

                                viewer.set_graph();

                                crate::OVERLAY.cloned().set(Box::new(viewer));

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
                                        let card = card.clone();
                                        let closure = _closure.clone();
                                        let done = is_done.clone();
                                        spawn(async move {
                                            closure(card.card.clone());
                                        });

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
}
