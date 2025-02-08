use super::{card_selector::CardSelector, itemselector::ItemSelector, OverlayEnum};
use crate::{overlays::card_selector::MyClosure, APP};
use dioxus::prelude::*;
use speki_core::{
    collection::{Collection, CollectionId, DynCard},
    Card,
};
use speki_dto::Item;
use speki_web::CardEntry;
use std::{fmt::Display, sync::Arc};
use tracing::info;

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
            let card = APP.read().load_card(id).await.card.read().print().await;
            format!("{card}")
        }
        DynCard::Instances(id) => {
            let card = APP.read().load_card(id).await.card.read().print().await;
            format!("instances of {card}")
        }
        DynCard::Dependents(id) => {
            let card = APP.read().load_card(id).await.card.read().print().await;
            format!("dependents of {card}")
        }
        DynCard::RecDependents(id) => {
            let card = APP.read().load_card(id).await.card.read().print().await;
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

#[derive(Props, PartialEq, Clone)]
pub struct ColViewer {
    pub col: Collection,
    pub colname: Signal<String>,
    pub done: Signal<bool>,
    pub entries: Signal<Vec<DynEntry>>,
    pub overlay: Signal<Option<OverlayEnum>>,
    pub addnew: Signal<bool>,
}

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

        info!("debug 6");

        let selv = Self {
            col,
            colname,
            done: Signal::new_in_scope(false, ScopeId::APP),
            entries,
            overlay: Signal::new_in_scope(None, ScopeId::APP),
            addnew: Signal::new_in_scope(false, ScopeId::APP),
        };

        selv
    }
}

#[component]
pub fn ChoiceRender(props: ColViewer) -> Element {
    let overlay = props.overlay.clone();
    let addnew = props.addnew;
    let entries = props.entries.clone();
    let col_id = props.col.id;

    rsx! {

        div {
            class: "flex flex-col",


        button {
            onclick: move |_|{
                addnew.clone().set(false);
            },
            "go back"
        }
        button {
            onclick: move |_|{
                overlay.clone().set(Some(OverlayEnum::CardSelector(entry_selector::card(entries))));
            },
            "card"
        }
        button {
            onclick: move |_|{
                overlay.clone().set(Some(OverlayEnum::CardSelector(entry_selector::instances(entries))));
            },
            "instance"
        }
        button {
            onclick: move |_|{
                overlay.clone().set(Some(OverlayEnum::CardSelector(entry_selector::dependencies(entries))));
            },
            "recursive dependents"
        }
        button {
            onclick: move |_|{
                spawn(async move {
                    overlay.clone().set(Some(OverlayEnum::ColSelector(entry_selector::collection(entries, col_id).await)));
                });

            },
            "collection"
        }

        }
    }
}

#[component]
pub fn ColViewRender(props: ColViewer) -> Element {
    let mut name = props.colname.clone();
    let cards = props.entries.clone();
    let selv = props.clone();
    let selv2 = props.clone();
    let addnew = props.addnew.clone();

    if addnew.cloned() {
        return ChoiceRender(props);
    }

    rsx! {
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


            button {
                onclick: move |_| {
                    addnew.clone().set(true);
                },

                "add new"

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
            }
        }
    }
}

mod entry_selector {
    use super::*;

    pub fn dependencies(entries: Signal<Vec<DynEntry>>) -> CardSelector {
        let f = MyClosure::new(move |card: CardEntry| {
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

        CardSelector::dependency_picker(f).with_title("all dependents of...".to_string())
    }

    pub fn instances(entries: Signal<Vec<DynEntry>>) -> CardSelector {
        let f = MyClosure::new(move |card: CardEntry| {
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

        CardSelector::class_picker(f)
    }

    pub async fn collection(
        entries: Signal<Vec<DynEntry>>,
        exclude: CollectionId,
    ) -> ItemSelector<Collection> {
        let mut cols = APP.read().load_collections().await;
        cols.retain(|col| col.id != exclude);
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

        ItemSelector::new(cols, Arc::new(f))
    }

    pub fn card(entries: Signal<Vec<DynEntry>>) -> CardSelector {
        let f = MyClosure::new(move |card: CardEntry| {
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

        CardSelector::dependency_picker(f).with_title("pick card".to_string())
    }
}
