use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    pin::Pin,
    sync::Arc,
    time::Duration,
};

use dioxus::prelude::*;
use speki_core::{
    card::{bigrams, normalize_string, CardId},
    cardfilter::CardFilter,
    collection::DynCard,
    set::{SetExpr, SetExprDiscriminants},
    Card, CardProperty,
};
use speki_web::Node;
use tracing::info;
use uuid::Uuid;

use crate::{
    append_overlay,
    pages::{ExprEditor, InputEditor, RenderExpr},
    set_overlay,
};

use crate::{
    components::{CardTy, FilterComp, FilterEditor},
    overlays::cardviewer::CardViewer,
    APP,
};

use super::OverlayEnum;

pub fn overlay_card_viewer() -> MyClosure {
    MyClosure::new(move |card: Signal<Card>| async move {
        let viewer = CardViewer::new_from_card(card).await;
        append_overlay(OverlayEnum::CardViewer(viewer));
    })
}

#[derive(Clone)]
pub enum MaybeEntry {
    Yes(Signal<Card>),
    No(CardId),
}

impl MaybeEntry {
    pub async fn entry(&mut self) -> Option<Signal<Card>> {
        let id = match self {
            Self::Yes(card) => return Some(card.clone()),
            Self::No(id) => id,
        };

        let card = APP.read().try_load_card(*id).await?;

        *self = Self::Yes(card.clone());
        Some(card)
    }
}

/*

idea: like a default search thing when you dont have anything in search bar

for example, if you click add dependency, and you dont have anything in search bar
the hidden search thing will be the front of the card that youre adding dependency to

this will mean a lot of times you dont have to even search

*/

#[derive(Props, Clone)]
pub struct CardSelector {
    pub title: Option<String>,
    pub search: Signal<String>,
    pub on_card_selected: MyClosure,
    pub allow_new: bool,
    pub dependents: Signal<Vec<Node>>,
    pub allowed_cards: Signal<Vec<CardTy>>,
    pub filtereditor: FilterEditor,
    pub filtermemo: Memo<Option<CardFilter>>,
    pub collection: ExprEditor,
    pub edit_collection: bool,
    pub cards: Resource<Vec<Signal<Card>>>,
    pub col_cards: Resource<BTreeMap<Uuid, Signal<MaybeEntry>>>,
    pub default_search: Signal<Option<String>>,
    pub forbidden_cards: Signal<BTreeSet<CardId>>,
}

impl Default for CardSelector {
    fn default() -> Self {
        Self::new(true, vec![])
    }
}

impl CardSelector {
    pub fn new_with_filter(with_memo: bool, allowed_cards: Vec<CardTy>, filter: SetExpr) -> Self {
        let allowed_cards = Signal::new_in_scope(allowed_cards, ScopeId::APP);
        let default_search: Signal<Option<String>> = Signal::new_in_scope(None, ScopeId::APP);
        let forbidden_cards: Signal<BTreeSet<CardId>> =
            Signal::new_in_scope(Default::default(), ScopeId::APP);

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
                        isolated: editor.isolated.get_value(),
                    })
                })
            }
        });

        let search = Signal::new_in_scope(String::new(), ScopeId::APP);
        let normalized_search = ScopeId::APP.in_runtime(move || {
            use_memo(move || {
                let searched = search.read();
                match (default_search.read().as_ref(), searched.is_empty()) {
                    (Some(s), true) => normalize_string(s.as_str()),
                    (_, _) => normalize_string(&searched),
                }
            })
        });

        let collection: ExprEditor = ExprEditor::from(filter);
        let col_cards = collection.expanded();

        let allowed = allowed_cards.clone();
        let cards = ScopeId::APP.in_runtime(|| {
            let allowed = allowed.clone();
            let cards = col_cards.clone();
            let search = normalized_search.clone();
            use_resource(move || {
                let allowed_cards = allowed.clone();
                let search = search.cloned();

                async move {
                    let allowed_cards = allowed_cards.clone();
                    let mut filtered_cards: Vec<(u32, Signal<Card>)> = Default::default();

                    let Some(cards) = cards.as_ref() else {
                        info!("fuck! :O ");
                        return vec![];
                    };

                    info!("so many cards! {}", cards.len());

                    let sorted_cards: Vec<(u32, CardId)> = if search.chars().count() < 2 {
                        cards.iter().map(|x| (0, *x.0)).collect()
                    } else {
                        let mut matching_cards: BTreeMap<Uuid, u32> = BTreeMap::new();
                        let bigrams = bigrams(search.as_ref());

                        for bigram in bigrams {
                            let indices = APP
                                .read()
                                .inner()
                                .card_provider
                                .providers
                                .cards
                                .get_prop_cache(
                                    CardProperty::Bigram,
                                    format!("{}{}", bigram[0], bigram[1]),
                                );

                            for id in indices {
                                let id: Uuid = id.parse().unwrap();
                                if cards.contains_key(&id) {
                                    *matching_cards.entry(id).or_insert(0) += 1;
                                }
                            }
                        }

                        info!("sorting cards");
                        let mut sorted_cards: Vec<_> = matching_cards.into_iter().collect();
                        sorted_cards.sort_by(|a, b| b.1.cmp(&a.1));
                        sorted_cards.into_iter().map(|c| (c.1, c.0)).collect()
                    };

                    info!("{} cards sorted", sorted_cards.len());

                    for (matches, card) in sorted_cards {
                        let entry = cards.get(&card).unwrap();

                        #[allow(deprecated)]
                        let card = entry.write_silent().entry().await.unwrap();

                        if forbidden_cards.read().contains(&card.read().id()) {
                            continue;
                        }

                        if filtered_cards.len() > 100 {
                            break;
                        }

                        if allowed_cards.is_empty()
                            || allowed_cards
                                .read()
                                .contains(&CardTy::from_ctype(card.read().card_type()))
                        {
                            let flag = match filtermemo.cloned() {
                                Some(filter) => filter.filter(Arc::new(card.cloned())).await,
                                None => true,
                            };

                            if flag {
                                filtered_cards.push((matches, card));
                            }
                        }
                    }

                    info!("done filtering :)");

                    filtered_cards.sort_by(|a, b| {
                        let ord_key = b.0.cmp(&a.0);
                        if ord_key == std::cmp::Ordering::Equal {
                            let card_a = a.1.read();
                            let card_b = b.1.read();
                            card_a.name().len().cmp(&card_b.name().len())
                        } else {
                            ord_key
                        }
                    });

                    filtered_cards.into_iter().map(|x| x.1).collect()
                }
            })
        });

        info!("creating cardselector");
        Self {
            title: Some("select card".to_string()),
            edit_collection: true,
            search,
            on_card_selected: overlay_card_viewer(),
            cards,
            allow_new: false,
            dependents: Signal::new_in_scope(Default::default(), ScopeId::APP),
            allowed_cards,
            filtereditor,
            filtermemo,
            collection,
            col_cards,
            default_search,
            forbidden_cards,
        }
    }

    pub fn new(with_memo: bool, allowed_cards: Vec<CardTy>) -> Self {
        Self::new_with_filter(with_memo, allowed_cards, SetExpr::All)
    }

    pub fn with_set(self, set: SetExpr) -> Self {
        Self {
            collection: ExprEditor::from(set),
            ..self
        }
    }

    pub fn with_dyncards(mut self, dyns: Vec<DynCard>) -> Self {
        let leafs: Vec<InputEditor> = dyns.into_iter().map(|x| InputEditor::Leaf(x)).collect();
        self.collection.inputs.write().extend(leafs);
        self.collection.ty.set(SetExprDiscriminants::Union);
        self
    }

    pub fn with_edit_collection(self, edit_collection: bool) -> Self {
        Self {
            edit_collection,
            ..self
        }
    }

    pub fn ref_picker(fun: MyClosure, dependents: Vec<Node>, filter: SetExpr) -> Self {
        Self {
            title: Some("choose reference".to_string()),
            on_card_selected: fun,
            allow_new: true,
            dependents: Signal::new_in_scope(dependents, ScopeId(3)),
            ..Self::new_with_filter(false, vec![], filter)
        }
    }

    pub fn class_picker(f: MyClosure) -> Self {
        Self::new(false, vec![CardTy::Class])
            .with_title("pick class".into())
            .new_on_card_selected(f)
    }

    pub fn dependency_picker(f: MyClosure) -> Self {
        Self {
            title: Some("set dependency".to_string()),
            on_card_selected: f,
            allow_new: true,
            ..Self::new(false, vec![])
        }
    }

    pub fn new_on_card_selected(mut self, f: MyClosure) -> Self {
        self.on_card_selected = f;
        self
    }

    pub fn with_forbidden_cards(mut self, cards: impl IntoIterator<Item = CardId>) -> Self {
        self.forbidden_cards.write().extend(cards);
        self
    }

    pub fn with_default_search(mut self, search: String) -> Self {
        self.default_search.set(Some(search));
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
        self.title = Some(title);
        self
    }

    pub fn no_title(self) -> Self {
        Self {
            title: None,
            ..self
        }
    }

    pub fn with_allow_new(mut self, allow_new: bool) -> Self {
        self.allow_new = allow_new;
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
    pub Arc<Box<dyn Fn(Signal<Card>) -> Pin<Box<dyn Future<Output = ()> + 'static>> + 'static>>,
);

impl MyClosure {
    pub fn new<F, Fut>(func: F) -> Self
    where
        F: Fn(Signal<Card>) -> Fut + 'static,
        Fut: Future<Output = ()> + 'static,
    {
        MyClosure(Arc::new(Box::new(move |card| {
            Box::pin(func(card)) as Pin<Box<dyn Future<Output = ()>>>
        })))
    }

    pub async fn call(&self, card: Signal<Card>) {
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
    title: Option<String>,
    search: Signal<String>,
    on_card_selected: MyClosure,
    cards: Resource<Vec<Signal<Card>>>,
    allow_new: bool,
    dependents: Signal<Vec<Node>>,
    allowed_cards: Signal<Vec<CardTy>>,
    filtereditor: FilterEditor,
    filtermemo: Memo<Option<CardFilter>>,
    collection: ExprEditor,
    edit_collection: bool,
) -> Element {
    info!("render cardselector");
    let filter = filtermemo.cloned().unwrap_or_default();
    rsx! {
        div {
            class: "flex flex-row",

        div {
            class: "flex flex-col",
            if filtermemo.read().is_some() {
                FilterComp {editor: filtereditor}
            }

            if edit_collection {
                RenderExpr { filter, inputs: collection.inputs.clone(), ty: collection.ty.clone()}
            }
        }

        div {
            class: "h-screen flex flex-col w-full max-w-3xl",

            if let Some(title) = title {
                h1 {
                    class: "text-lg font-bold mb-4",
                    "{title}"
                }
            }

            div {
                if allow_new {
                    NewcardButton { on_card_selected: on_card_selected.clone(), dependents: dependents(), allowed_cards: allowed_cards.cloned(), search }
                }

                input {
                    class: "bg-white w-full max-w-md border border-gray-300 rounded-md p-2 mb-4 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                    value: "{search}",
                    oninput: move |evt| search.set(evt.value().clone()),
                }
            }

            TableRender {
                cards, on_card_selected
              }
            }
        }
    }
}

#[component]
fn NewcardButton(
    on_card_selected: MyClosure,
    dependents: Vec<Node>,
    allowed_cards: Vec<CardTy>,
    search: String,
) -> Element {
    let closure = Arc::new(on_card_selected.clone());
    rsx! {
        button {
            class: "bg-blue-500 text-white font-medium px-4 py-2 rounded-md hover:bg-blue-600 focus:outline-none focus:ring-2 focus:ring-blue-300 mr-10",
            onclick: move |_| {

                let closure = closure.clone();
                let hook = MyClosure::new(move |card: Signal<Card>| {
                    let closure = closure.clone();

                    async move {
                        closure.call(card).await;
                    }
                });

                let viewer = CardViewer::new()
                    .with_hook(hook)
                    .with_dependents(dependents.clone())
                    .with_allowed_cards(allowed_cards.clone())
                    .with_front_text(search.clone());

                set_overlay(Some(OverlayEnum::CardViewer(viewer)));

            },
            "new card"
        }
    }
}

#[component]
fn TableRender(cards: Resource<Vec<Signal<Card>>>, on_card_selected: MyClosure) -> Element {
    let closure = Arc::new(on_card_selected.clone());

    let filtered_cards: Vec<_> = cards
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .take(1000)
        .zip(std::iter::repeat_with(|| Arc::clone(&closure)))
        .map(|(card, closure)| (card.clone(), closure))
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
                        th { class: "border border-gray-300 px-4 py-2 w-1/24", "Ty" }
                    }
                }
                tbody {
                    for (card, _closure) in filtered_cards {
                        tr {
                            class: "hover:bg-gray-50 cursor-pointer",
                            onclick: move |_| {
                                info!("clicky");
                                let card = card.clone();
                                let closure = _closure.clone();
                                spawn(async move {
                                    closure.call(card).await;
                                });
                            },

                            td { class: "border border-gray-300 px-4 py-2 w-2/3", "{card}" }
                            td { class: "border border-gray-300 px-4 py-2 w-1/12", "{card.read().recall_rate().unwrap_or_default():.2}" }
                            td { class: "border border-gray-300 px-4 py-2 w-1/12", "{maybe_dur(card.read().maturity())}"}
                            td { class: "border border-gray-300 px-4 py-2 w-1/24", "{card.read().card_type().short_form()}" }
                        }
                    }
                }
            }
        }
    }
}

fn maybe_dur(dur: Option<Duration>) -> String {
    match dur {
        Some(dur) => format_dur(dur),
        None => format!("None"),
    }
}

fn format_dur(dur: Duration) -> String {
    let secs = dur.as_secs();

    let minute = 60;
    let hour = minute * 60;
    let day = hour * 24;
    let year = day * 365;

    if secs < minute {
        format!("{secs}s")
    } else if secs < hour {
        format!("{}m", secs / minute)
    } else if secs < day {
        format!("{}h", secs / hour)
    } else if secs < day * 1000 {
        format!("{}d", secs / day)
    } else if secs < year * 10 {
        format!("{:.2}y", secs as f32 / year as f32)
    } else if secs < year * 100 {
        format!("{:.1}y", secs as f32 / year as f32)
    } else {
        format!("{}y", secs / year)
    }
}
