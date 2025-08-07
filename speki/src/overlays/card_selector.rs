use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};

use dioxus::prelude::*;
use ledgerstore::PropertyCache;
use speki_core::{
    card::{bigrams, normalize_string, CardId},
    cardfilter::CardFilter,
    set::SetExpr,
    Card, CardProperty,
};
use tracing::info;
use uuid::Uuid;

use crate::{
    components::SectionWithTitle,
    pages::{reviewable_cards, ExprEditor, RenderExpr},
    pop_overlay, set_overlay,
};

use crate::{
    components::{CardTy, FilterComp, FilterEditor},
    overlays::cardviewer::CardViewer,
    APP,
};

use super::OverlayEnum;

pub fn overlay_card_viewer() -> MyClosure {
    MyClosure::new(move |card: CardId| OverlayEnum::new_edit_card(card).append())
}

#[derive(Debug, Clone)]
pub enum MaybeEntry {
    Yes(Arc<Card>),
    No(CardId),
}

impl MaybeEntry {
    pub fn entry(&mut self) -> Option<Arc<Card>> {
        let id = match self {
            Self::Yes(card) => return Some(card.clone()),
            Self::No(id) => id,
        };

        let card = APP.read().try_load_card(*id)?;

        *self = Self::Yes(card.clone());
        Some(card)
    }
}

type OnCardSelected = (MyClosure, bool); // if true, close the overlay

#[derive(Props, Clone, Debug)]
pub struct CardSelector {
    pub title: Option<String>,
    pub search: Signal<String>,
    pub on_card_selected: OnCardSelected,
    pub allow_new: bool,
    pub allowed_cards: Signal<Vec<CardTy>>,
    pub filtereditor: FilterEditor,
    pub filtermemo: Memo<Option<CardFilter>>,
    pub collection: ExprEditor,
    pub edit_collection: bool,
    pub cards: Memo<Vec<Arc<Card>>>,
    pub col_cards: Memo<BTreeMap<Uuid, Signal<MaybeEntry>>>,
    pub default_search: Signal<Option<String>>,
    pub forbidden_cards: Signal<BTreeSet<CardId>>,
    pub instance_of: Option<CardId>,
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
                Memo::new(|| None)
            } else {
                Memo::new(move || {
                    Some(CardFilter {
                        recall: editor.recall.get_value(),
                        rec_recall: editor.rec_recall.get_value(),
                        stability: editor.stability.get_value(),
                        rec_stability: editor.rec_stability.get_value(),
                        suspended: editor.suspended.get_value(),
                        lapses: editor.lapses.get_value(),
                        needs_work: editor.needs_work.get_value(),
                    })
                })
            }
        });

        let search = Signal::new_in_scope(String::new(), ScopeId::APP);
        let normalized_search = ScopeId::APP.in_runtime(move || {
            Memo::new(move || {
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
            Memo::new(move || {
                let allowed_cards = allowed.clone();
                let search = search.cloned();

                let allowed_cards = allowed_cards.clone();
                let mut filtered_cards: Vec<(u32, Arc<Card>)> = Default::default();

                let cards = cards.read();

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
                            .get_prop_cache(PropertyCache::new(
                                CardProperty::Bigram,
                                format!("{}{}", bigram[0], bigram[1]),
                            ));

                        for id in indices {
                            if cards.contains_key(&id) {
                                *matching_cards.entry(id).or_insert(0) += 1;
                            }
                        }
                    }

                    if matching_cards.len() < 100 {
                        for card in cards.keys().take(100) {
                            if !matching_cards.contains_key(card) {
                                matching_cards.insert(card.to_owned(), 0);
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
                    let entry = match cards.get(&card) {
                        Some(card) => card,
                        None => {
                            tracing::error!("missing card: {}", card);
                            continue;
                        }
                    };

                    #[allow(deprecated)]
                    let card = match entry.write_silent().entry() {
                        Some(card) => card,
                        None => continue,
                    };

                    if forbidden_cards.read().contains(&card.id()) {
                        continue;
                    }

                    if filtered_cards.len() > 100 {
                        break;
                    }

                    if allowed_cards.is_empty()
                        || allowed_cards
                            .read()
                            .contains(&CardTy::from_ctype(card.card_type()))
                    {
                        let flag = match filtermemo.cloned() {
                            Some(filter) => filter.filter(card.clone()),
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
                        let card_a = &a.1;
                        let card_b = &b.1;
                        card_a.name().len().cmp(&card_b.name().len())
                    } else {
                        ord_key
                    }
                });

                filtered_cards.into_iter().map(|x| x.1).collect()
            })
        });

        info!("creating cardselector");
        Self {
            title: Some("select card".to_string()),
            edit_collection: true,
            search,
            on_card_selected: (overlay_card_viewer(), false),
            cards,
            allow_new: false,
            allowed_cards,
            filtereditor,
            filtermemo,
            collection,
            col_cards,
            default_search,
            forbidden_cards,
            instance_of: None,
        }
    }

    pub fn new(with_memo: bool, allowed_cards: Vec<CardTy>) -> Self {
        Self::new_with_filter(with_memo, allowed_cards, SetExpr::All)
    }

    pub fn with_instance_of(self, class: CardId) -> Self {
        Self {
            instance_of: Some(class),
            ..self
        }
    }

    pub fn with_set(mut self, set: SetExpr) -> Self {
        let editor: ExprEditor = ExprEditor::from(set);
        self.collection.inputs.set(editor.inputs.cloned());
        self.collection.ty.set(editor.ty.cloned());
        self
    }

    pub fn with_edit_collection(self, edit_collection: bool) -> Self {
        Self {
            edit_collection,
            ..self
        }
    }

    pub fn ref_picker(fun: MyClosure, filter: SetExpr) -> Self {
        Self {
            title: Some("choose reference".to_string()),
            on_card_selected: (fun, true),
            allow_new: true,
            ..Self::new_with_filter(false, vec![], filter)
        }
    }

    pub fn class_picker(f: MyClosure) -> Self {
        Self::new(false, vec![CardTy::Class])
            .with_title("pick class".into())
            .new_on_card_selected(f, true)
    }

    pub fn dependency_picker(f: MyClosure) -> Self {
        Self {
            title: Some("set dependency".to_string()),
            on_card_selected: (f, true),
            allow_new: true,
            ..Self::new(false, vec![])
        }
    }

    pub fn new_on_card_selected(mut self, f: MyClosure, close_popup: bool) -> Self {
        self.on_card_selected = (f, close_popup);
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
pub struct MyClosure(pub Arc<Box<dyn Fn(CardId)>>);

impl std::fmt::Debug for MyClosure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("MyClosure").finish()
    }
}

impl MyClosure {
    pub fn new<F>(func: F) -> Self
    where
        F: Fn(CardId) + 'static,
    {
        MyClosure(Arc::new(Box::new(func)))
    }

    pub fn call(&self, card: CardId) {
        (self.0)(card)
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
    on_card_selected: OnCardSelected,
    cards: Memo<Vec<Arc<Card>>>,
    allow_new: bool,
    allowed_cards: Signal<Vec<CardTy>>,
    filtereditor: FilterEditor,
    filtermemo: Memo<Option<CardFilter>>,
    collection: ExprEditor,
    edit_collection: bool,
    instance_of: Option<CardId>,
    #[props(default = false)] reviewable: bool,
) -> Element {
    info!("render cardselector");
    let filter = filtermemo.cloned().unwrap_or_default();
    let has_cards = !cards.read().is_empty();

    let expr = SetExpr::try_from(collection.clone());

    let review_title = if expr.is_err() {
        Some("invalid set")
    } else if !has_cards {
        Some("no cards to review")
    } else {
        None
    };

    let disabled = review_title.is_some();
    let review_title = review_title.unwrap_or_default();
    let review_filter = filter.clone();

    rsx! {
        div {
            class: "flex flex-row",
                div {
                class: "flex flex-col",

                if filtermemo.read().is_some() {
                    div {
                        class: "mb-6",
                        SectionWithTitle {
                            title: "Filter".to_string(),
                            FilterComp { editor: filtereditor }
                        }
                    }
                }

                if edit_collection {
                    div {
                        class: "mb-6",
                        SectionWithTitle {
                            title: "Set".to_string(),
                            RenderExpr { filter, inputs: collection.inputs.clone(), ty: collection.ty.clone() }
                        }
                    }
                }

                if reviewable {
                    div {
                        class: "mb-6",
                        button {
                            class: "{crate::styles::READ_BUTTON} w-full",
                            disabled,
                            title: review_title,
                            onclick: move |_| {
                                if let Ok(expr) = expr.clone() {
                                    if let Some(cards) = reviewable_cards(expr, Some(review_filter.clone())) {
                                        OverlayEnum::new_review(cards).append();
                                    } else {
                                        debug_assert!(false);
                                    }
                                }
                            },
                            "review"
                        }
                    }
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
                    NewcardButton { on_card_selected: on_card_selected.clone(), allowed_cards: allowed_cards.cloned(), search, instance_of }
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
    on_card_selected: OnCardSelected,
    allowed_cards: Vec<CardTy>,
    search: String,
    instance_of: Option<CardId>,
) -> Element {
    let closure = Arc::new(on_card_selected.clone());
    rsx! {
        button {
            class: "{crate::styles::CREATE_BUTTON}",
            onclick: move |_| {

                let closure = closure.clone();
                let hook = MyClosure::new(move |card: CardId| {
                let closure = closure.clone();
                closure.0.call(card);
                if closure.1 {
                    pop_overlay();
                }
                });

                let mut viewer = CardViewer::new()
                    .with_hook(hook)
                    .with_allowed_cards(allowed_cards.clone())
                    .with_front_text(search.clone());

                if let Some(class) = instance_of {
                    viewer = viewer.with_class(class);
                }

                dbg!(&viewer);


                set_overlay(Some(OverlayEnum::CardViewer(viewer)));

            },
            "new card"
        }
    }
}

#[component]
fn TableRender(cards: Memo<Vec<Arc<Card>>>, on_card_selected: OnCardSelected) -> Element {
    let closure = Arc::new(on_card_selected.clone());

    let filtered_cards: Vec<_> = cards
        .cloned()
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
                                closure.0.call(card.id());
                                if closure.1 {
                                    pop_overlay();
                                }
                            },

                            td { class: "border border-gray-300 px-4 py-2 w-2/3", "{card}" }
                            td { class: "border border-gray-300 px-4 py-2 w-1/12", "{card.recall_rate().unwrap_or_default():.2}" }
                            td { class: "border border-gray-300 px-4 py-2 w-1/12", "{maybe_dur(card.maturity())}"}
                            td { class: "border border-gray-300 px-4 py-2 w-1/24", "{card.card_type().short_form()}" }
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
