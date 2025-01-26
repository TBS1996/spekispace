use super::{card_selector::CardSelector, itemselector::ItemSelector, OverlayEnum};
use crate::{
    overlays::{
        card_selector::{CardSelectorRender, MyClosure},
        itemselector::ItemSelectorRender,
    },
    APP,
};
use dioxus::prelude::*;
use speki_core::{
    collection::{Collection, CollectionId, DynCard},
    Card,
};
use speki_dto::Item;
use std::{fmt::Display, sync::Arc};

/*

two main parts

one is the list of the collection

the other is the option to add more cards


first one has two extra options which is like, show expanded, so expand the dynamic ones, or ideally it'd be like a expand arrow thing

the other one has maybe like tabs ?

one tab is just search for a specific card and choose it
other is the various dynamic things like, choose dependents of cards, choose other collection etc...


*/

#[derive(Clone, Eq, PartialEq)]
pub struct DynEntry {
    name: String,
    dy: DynCard,
    cards: Vec<Arc<Card>>,
}

impl Display for DynEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let DynCard::Card(_) = &self.dy {
            write!(f, "{}", &self.name)
        } else {
            let qty = self.cards.len();
            write!(f, "{} ({qty})", &self.name)
        }
    }
}

async fn name(dy: DynCard) -> String {
    match dy {
        DynCard::Card(id) => {
            let card = APP.read().load_card(id).await;
            format!("{card}")
        }
        DynCard::Instances(id) => {
            let card = APP.read().load_card(id).await;
            format!("instances of {card}")
        }
        DynCard::Dependents(id) => {
            let card = APP.read().load_card(id).await;
            format!("dependents of {card}")
        }
        DynCard::RecDependents(id) => {
            let card = APP.read().load_card(id).await;
            format!("rec dependents of {card}")
        }
        DynCard::Collection(id) => {
            let col = APP.read().load_collection(id).await;
            format!("collection: {col}")
        }
    }
}

impl DynEntry {
    async fn new(dy: DynCard) -> Self {
        let provider = APP.read().inner().card_provider();

        let name = name(dy.clone()).await;

        let cards = dy.evaluate(provider, Default::default()).await;

        Self { name, dy, cards }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum DynTab {
    Card,
    Instance,
    RecDependents,
    Collection,
}

impl DynTab {
    fn next(&mut self) {
        *self = match self {
            Self::Card => Self::Instance,
            Self::Instance => Self::RecDependents,
            Self::RecDependents => Self::Collection,
            Self::Collection => Self::Card,
        };
    }
}

#[derive(Props, PartialEq, Clone)]
pub struct ColViewer {
    pub col: Collection,
    pub colname: Signal<String>,
    pub done: Signal<bool>,
    pub entries: Signal<Vec<DynEntry>>,
    pub cardselector: CardSelector,
    pub colselector: ItemSelector<Collection>,
    pub instance_selector: CardSelector,
    pub dependents_selector: CardSelector,
    pub dynty: Signal<DynTab>,
    pub overlay: Signal<Option<OverlayEnum>>,
}

use tracing::info;

impl ColViewer {
    pub async fn new(id: CollectionId) -> Self {
        info!("debug 1");
        let col = APP.read().load_collection(id).await;
        let colname = Signal::new_in_scope(col.name.clone(), ScopeId::APP);
        let mut entries = vec![];
        for dy in col.dyncards.clone() {
            entries.push(DynEntry::new(dy).await);
        }

        let entries = Signal::new_in_scope(entries, ScopeId::APP);

        let f = MyClosure::new(move |card: Arc<Card>| {
            let entries = entries.clone();
            async move {
                let mut inner = entries.cloned();
                let entry = DynEntry::new(DynCard::RecDependents(card.id())).await;
                let contains = inner.iter().any(|inentry| inentry.dy == entry.dy);

                if !contains {
                    inner.push(entry);
                    entries.clone().set(inner);
                }
            }
        });
        info!("debug 2");

        let depselector = CardSelector::dependency_picker(f)
            .await
            .with_title("all dependents of...".to_string());

        let f = MyClosure::new(move |card: Arc<Card>| {
            let entries = entries.clone();
            let mut inner = entries.cloned();
            async move {
                let entry = DynEntry::new(DynCard::Card(card.id())).await;
                let contains = inner.iter().any(|inentry| inentry.dy == entry.dy);

                if !contains {
                    inner.push(entry);
                    entries.clone().set(inner);
                }
            }
        });

        info!("debug 3");

        let cardselector = CardSelector::dependency_picker(f)
            .await
            .with_title("pick card".to_string());

        let f = MyClosure::new(move |card: Arc<Card>| {
            let entries = entries.clone();
            async move {
                let mut inner = entries.cloned();
                let entry = DynEntry::new(DynCard::Instances(card.id())).await;
                let contains = inner.iter().any(|inentry| inentry.dy == entry.dy);

                if !contains {
                    inner.push(entry);
                    entries.clone().set(inner);
                }
            }
        });

        let instance_selector = CardSelector::class_picker(f).await;

        let mut cols = APP.read().load_collections().await;
        cols.retain(|_col| _col.id != col.id);
        info!("debug 4");

        let f = Box::new(move |col: Collection| {
            let entries = entries.clone();
            spawn(async move {
                let mut inner = entries.cloned();
                let entry = DynEntry::new(DynCard::Collection(col.id())).await;
                let contains = inner.iter().any(|inentry| inentry.dy == entry.dy);

                if !contains {
                    inner.push(entry);
                    entries.clone().set(inner);
                }
            });
        });

        let colselector = ItemSelector::new(cols, Arc::new(f));
        info!("debug 5");

        let selv = Self {
            col,
            colname,
            colselector,
            done: Signal::new_in_scope(false, ScopeId::APP),
            instance_selector,
            entries,
            cardselector,
            dependents_selector: depselector,
            dynty: Signal::new_in_scope(DynTab::Collection, ScopeId::APP),
            overlay: Signal::new_in_scope(None, ScopeId::APP),
        };
        info!("debug 6");

        selv
    }
}

#[component]
pub fn ColViewRender(props: ColViewer) -> Element {
    let mut name = props.colname.clone();
    let cards = props.entries.clone();
    let selector = props.cardselector.clone();
    let inselector = props.instance_selector.clone();
    let depselector = props.dependents_selector.clone();
    let colselector = props.colselector.clone();
    let selv = props.clone();
    let selv2 = props.clone();
    let ty = props.dynty.clone();
    let ty2 = props.dynty.clone();
    let overlay = props.overlay.clone();

    rsx! {

        div {
            h1 {"{ty2:?}"}
        }


        button {
            class: "inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base",
            onclick: move |_| {
                let mut _ty = ty.cloned();
                _ty.next();
                ty.clone().set(_ty);
            },
            "change dynty"
        }

        button {
            class: "inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base",
            onclick: move |_| {
                let selv = selv.clone();

                let name = selv.colname.cloned();
                let entries = selv.entries.cloned();
                let mut col = selv.col.clone();
                col.name = name;
                col.dyncards = entries.into_iter().map(|entry|entry.dy).collect();

                spawn(async move {
                    APP.read().save_collection(col).await;
                    selv.done.clone().set(true);
                });


            },
            "save"
        }


        button {
            class: "inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base",
            onclick: move |_| {
                let selv = selv2.clone();
                spawn(async move {
                    APP.read().delete_collection(selv.col.id).await;
                    selv.done.clone().set(true);
                });


            },
            "delete"
        }

        div {
            div {
                class: "mb-10",
                input {
                    value: "{name}",
                    oninput: move |evt| name.set(evt.value()),
                }
            }

            div {
                class: "flex flex-row",
                div {
                    for card in cards.read().clone() {
                        div {
                            class: "flex flex-row mb-2",

                            button {
                                onclick: move |_| {
                                    let mut inner = cards.cloned();
                                    inner.retain(|entry|entry.dy != card.dy);
                                    cards.clone().set(inner);
                                },

                                "❌"

                            }

                            h1 {
                                "{card}"
                            }
                        }
                    }
                }

                div {
                    class: "ml-20",

                    match ty.cloned() {
                        DynTab::Card => rsx!{
                            CardSelectorRender {
                                title: selector.title.clone(),
                                search: selector.search.clone(),
                                on_card_selected: selector.on_card_selected.clone(),
                                cards: selector.cards.clone(),
                                allow_new: selector.allow_new.clone(),
                                done: selector.done.clone(),
                                filter: selector.filter.clone(),
                                dependents: selector.dependents.clone(),
                                allowed_cards: selector.allowed_cards.clone(),
                                filtereditor: selector.filtereditor.clone(),
                                filtermemo: selector.filtermemo.clone(),
                                overlay: overlay.clone(),
                            }
                        },
                        DynTab::Instance => rsx!{
                            CardSelectorRender {
                                title: inselector.title.clone(),
                                search: inselector.search.clone(),
                                on_card_selected: inselector.on_card_selected.clone(),
                                cards: inselector.cards.clone(),
                                allow_new: inselector.allow_new.clone(),
                                done: inselector.done.clone(),
                                filter: inselector.filter.clone(),
                                dependents: inselector.dependents.clone(),
                                allowed_cards: inselector.allowed_cards.clone(),
                                filtereditor: inselector.filtereditor.clone(),
                                filtermemo: inselector.filtermemo.clone(),
                                overlay: overlay.clone(),
                            }
                        },
                        DynTab::RecDependents => rsx!{
                            CardSelectorRender {
                                title: depselector.title.clone(),
                                search: depselector.search.clone(),
                                on_card_selected: depselector.on_card_selected.clone(),
                                cards: depselector.cards.clone(),
                                allow_new: depselector.allow_new.clone(),
                                done: depselector.done.clone(),
                                filter: depselector.filter.clone(),
                                dependents: depselector.dependents.clone(),
                                allowed_cards: depselector.allowed_cards.clone(),
                                filtereditor: depselector.filtereditor.clone(),
                                filtermemo: depselector.filtermemo.clone(),
                                overlay: overlay.clone(),
                            }
                        },
                        DynTab::Collection => rsx!{
                            ItemSelectorRender{
                                items: colselector.items.clone(),
                                on_selected: colselector.on_selected.clone(),
                                done: colselector.done.clone(),
                            }
                        },
                    }
                }
            }
        }
    }
}
