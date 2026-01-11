use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fs,
    ops::ControlFlow,
    sync::Arc,
    time::Duration,
};

use dioxus::prelude::*;
use ledgerstore::{LedgerEvent, PropertyCache};
use simpletime::timed;
use speki_core::{
    card::{bigrams, normalize_string, CardId},
    cardfilter::{CardFilter, HistoryFilter, MetaFilter},
    current_time,
    ledger::CardEvent,
    reviewable_cards,
    set::SetExpr,
    Card, CardProperty, Config,
};
use tracing::info;
use uuid::Uuid;

use crate::{
    components::SectionWithTitle,
    pages::{ExprEditor, RenderExpr},
    pop_overlay, set_overlay,
    utils::{handle_card_event_error, App},
    RemoteUpdate,
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

type OnCardSelected = (MyClosure, bool); // if true, close the overlay

#[derive(Props, Clone)]
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
    pub col_cards: Memo<BTreeSet<Uuid>>,
    pub default_search: Signal<Option<String>>,
    pub forbidden_cards: Signal<BTreeSet<CardId>>,
    pub instance_of: Option<CardId>,
}

struct CardFilteringState {
    allowed_cards: Vec<CardTy>,
    forbidden_cards: BTreeSet<Uuid>,
    filtered_cards: Vec<(u32, Arc<Card>)>,
    filter: Option<CardFilter>,
    app: App,
    time: Duration,
}

impl CardFilteringState {
    fn new(
        allowed_cards: Vec<CardTy>,
        forbidden_cards: BTreeSet<Uuid>,
        filter: Option<CardFilter>,
    ) -> Self {
        Self {
            allowed_cards,
            forbidden_cards,
            filtered_cards: Default::default(),
            filter,
            app: APP.read().clone(),
            time: current_time(),
        }
    }

    fn evaluate(&self, card: CardId) -> ControlFlow<(), Option<Arc<Card>>> {
        if self.forbidden_cards.contains(&card) {
            return ControlFlow::Continue(None);
        };

        if self.filtered_cards.len() > 100 {
            return ControlFlow::Break(());
        }

        let Some(card) = self.app.load(card) else {
            return ControlFlow::Continue(None);
        };

        if !self.allowed_cards.is_empty()
            && !self
                .allowed_cards
                .contains(&CardTy::from_ctype(card.card_type()))
        {
            return ControlFlow::Continue(None);
        }

        let flag = match &self.filter {
            Some(filter) => filter.filter_old(card.clone(), self.time),
            None => true,
        };

        if flag {
            ControlFlow::Continue(Some(card))
        } else {
            ControlFlow::Continue(None)
        }
    }

    fn evaluate_cards(&mut self, cards: Vec<(u32, CardId)>) -> Vec<Arc<Card>> {
        for (matches, card) in cards {
            match self.evaluate(card) {
                ControlFlow::Continue(Some(card)) => {
                    self.filtered_cards.push((matches, card));
                }
                ControlFlow::Continue(None) => continue,
                ControlFlow::Break(_) => break,
            }
        }

        self.filtered_cards.sort_by(|a, b| {
            let ord_key = b.0.cmp(&a.0);
            if ord_key == std::cmp::Ordering::Equal {
                let card_a = &a.1;
                let card_b = &b.1;
                card_a.name().len().cmp(&card_b.name().len())
            } else {
                ord_key
            }
        });

        self.filtered_cards.iter().map(|x| x.1.clone()).collect()
    }
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
                        history: HistoryFilter {
                            recall: editor.recall.get_value(),
                            rec_recall: editor.rec_recall.get_value(),
                            stability: editor.stability.get_value(),
                            rec_stability: editor.rec_stability.get_value(),
                            lapses: editor.lapses.get_value(),
                        },
                        meta: MetaFilter {
                            suspended: editor.suspended.get_value(),
                            needs_work: editor.needs_work.get_value(),
                        },
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

                let cards = cards.read();

                info!("so many cards! {}", cards.len());

                let sorted_cards: Vec<(u32, CardId)> = if search.chars().count() < 2 {
                    cards.iter().map(|x| (0, *x)).collect()
                } else {
                    let mut matching_cards: BTreeMap<Uuid, u32> = BTreeMap::new();
                    let bigrams = bigrams(search.as_ref());

                    for bigram in bigrams {
                        let indices = APP.read().get_prop_cache(PropertyCache::new(
                            CardProperty::Bigram,
                            format!("{}{}", bigram[0], bigram[1]),
                        ));

                        for id in indices {
                            if cards.contains(&id) {
                                *matching_cards.entry(id).or_insert(0) += 1;
                            }
                        }
                    }

                    if matching_cards.len() < 100 {
                        for card in cards.iter().take(100) {
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

                let mut filter_state = CardFilteringState::new(
                    allowed_cards.cloned(),
                    forbidden_cards.cloned(),
                    filtermemo.cloned(),
                );

                timed!(filter_state.evaluate_cards(sorted_cards))
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
    #[props(default = false)] update_remote_button: bool,
) -> Element {
    info!("render cardselector");
    let filter = filtermemo.cloned().unwrap_or_default();
    let has_cards = !cards.read().is_empty();

    let expr = SetExpr::try_from(collection.clone());
    let expr2 = expr.clone();
    let expr3 = expr.clone();

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

    //let current_commit: Option<String> = None; //APP.read().inner().provider.cards.current_commit();
    //let latest_commit: Option<String> = None; // = APP.read().inner().provider.cards.latest_upstream_commit();

    let secret_export = search.read().contains("!export!");
    let secret_delete = search.read().contains("!delete!");

    let update_available = use_context::<RemoteUpdate>().latest_commit();

    rsx! {
        div {
            class: "flex flex-row",
                div {
                class: "flex flex-col",

                if let Some(latest_commit) = update_available.clone() {
                    button {
                        class: "{crate::styles::CREATE_BUTTON} px-3 py-2",
                        onclick: move |_| {
                            if let Err(err) = APP.read().modify_card(LedgerEvent::SetUpstream { commit: latest_commit.clone(), upstream_url: Config::upstream_url()}) {
                                handle_card_event_error(err);
                            } else {
                                use_context::<RemoteUpdate>().clear()
                            }
                        },
                        "update remote 🔃"
                    }

                }

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
                                    if let Some(cards) = reviewable_cards(APP.read().card_provider(), expr.clone(), Some(review_filter.clone())) {
                                        OverlayEnum::new_review(cards, expr, Some(review_filter.clone())).append();
                                    } else {
                                        debug_assert!(false);
                                    }
                                }
                            },
                            "review"
                        }
                        if secret_delete {
                            button {
                                class: "{crate::styles::DELETE_BUTTON} w-full",
                                onclick: move |_| {
                                    if let Ok(expr) = expr3.clone() {
                                        let item_set = expr.to_set();
                                        if let Err(e) = APP.read().card_provider().delete_set(item_set) {
                                            handle_card_event_error(e);
                                        }
                                    }
                                },
                                "delete all"
                            }
                        }
                        if secret_export {
                            button {
                                class: "{crate::styles::READ_BUTTON} w-full",
                                disabled,
                                title: review_title,
                                onclick: move |_| {
                                    if let Ok(expr) = expr2.clone() {
                                        let card_ids: HashSet<CardId> = APP.read().eval_expr(&expr).into_iter().collect();

                                        let mut all_recursive_dependencies: HashSet<CardId> = card_ids
                                            .iter()
                                            .map(|id| APP.read().dependencies_recursive(*id))
                                            .flatten()
                                            .collect();

                                        all_recursive_dependencies.extend(card_ids);

                                        let mut to_export: Vec<Card> = vec![];

                                        for card in all_recursive_dependencies {
                                            let card = APP.read().load(card).unwrap();
                                            if !card.is_remote() {
                                                to_export.push(Arc::unwrap_or_clone(card));
                                            }
                                        }

                                        let sorted = Card::transitive_sort(to_export).unwrap();
                                        let mut events: Vec<CardEvent> = vec![];

                                        for card in sorted {
                                            events.extend(Arc::unwrap_or_clone(card.clone_base()).into_events());
                                        }

                                        let Some(folder) = rfd::FileDialog::new()
                                            .set_directory(dirs::home_dir().unwrap())
                                            .pick_folder() else {
                                                return;
                                            };

                                        fs::create_dir_all(&folder).unwrap();

                                        let qty = events.len();

                                        for (idx, event) in events.into_iter().enumerate() {
                                            use std::io::Write;
                                            let s: String = serde_json::to_string_pretty(&event).unwrap();
                                            let name = format!("{:06}", idx);
                                            let path = folder.join(name);
                                            let mut f = fs::File::create(&path).unwrap();
                                            f.write_all(&mut s.into_bytes()).unwrap();
                                        }

                                        OverlayEnum::new_notice(format!("exported {qty} cards")).append();
                                    }
                                },
                                "export"
                            }
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
        .take(500)
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
                        th { class: "border border-gray-300 px-4 py-2 w-1/24", "Ty" }
                        th { class: "border border-gray-300 px-4 py-2 w-23/24", "Front" }
                        //th { class: "border border-gray-300 px-4 py-2 w-1/12", "Recall" }
                        //th { class: "border border-gray-300 px-4 py-2 w-1/12", "Stability" }
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

                            td { class: "border border-gray-300 px-4 py-2 w-1/24", "{card.card_type().short_form()}" }
                            td { class: "border border-gray-300 px-4 py-2 w-23/24", "{card}" }
                            //td { class: "border border-gray-300 px-4 py-2 w-1/12", "{card.recall_rate().unwrap_or_default():.2}" }
                            //td { class: "border border-gray-300 px-4 py-2 w-1/12", "{maybe_dur(card.maturity())}"}
                        }
                    }
                }
            }
        }
    }
}

fn _maybe_dur(dur: Option<Duration>) -> String {
    match dur {
        Some(dur) => _format_dur(dur),
        None => format!("None"),
    }
}

fn _format_dur(dur: Duration) -> String {
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
