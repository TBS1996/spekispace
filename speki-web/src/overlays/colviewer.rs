use std::{fmt::Display, sync::Arc};

use dioxus::prelude::*;
use speki_core::{
    collection::{Collection, CollectionId, DynCard},
    Card,
};

use crate::{components::Komponent, APP};

use super::{card_selector::CardSelector, Overlay};

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
struct DynEntry {
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

impl DynEntry {
    async fn new(dy: DynCard) -> Self {
        let id = dy.id();
        let card = APP.read().load_card(id).await;
        let provider = APP.read().inner().card_provider();

        let name = match dy {
            DynCard::Card(_) => format!("{card}"),
            DynCard::Instances(_) => format!("instances of {card}"),
            DynCard::Dependents(_) => format!("dependents of {card}"),
            DynCard::RecDependents(_) => format!("rec deps of {card}"),
        };
        let cards = dy.evaluate(provider).await;

        Self { name, dy, cards }
    }
}

#[derive(Debug, Clone, Copy)]
enum DynTab {
    Card,
    Instance,
}

#[derive(Clone)]
pub struct ColViewer {
    pub col: Collection,
    pub colname: Signal<String>,
    pub done: Signal<bool>,
    pub entries: Signal<Vec<DynEntry>>,
    pub cardselector: CardSelector,
    pub instance_selector: CardSelector,
    pub dynty: Signal<DynTab>,
}

impl ColViewer {
    pub async fn new(id: CollectionId) -> Self {
        let col = APP.read().load_collection(id).await;
        let colname = Signal::new_in_scope(col.name.clone(), ScopeId::APP);
        let mut entries = vec![];
        for dy in col.dyncards.clone() {
            entries.push(DynEntry::new(dy).await);
        }

        let entries = Signal::new_in_scope(entries, ScopeId::APP);

        let f = Box::new(move |card: Arc<Card>| {
            let entries = entries.clone();
            spawn(async move {
                let mut inner = entries.cloned();
                let entry = DynEntry::new(DynCard::Card(card.id())).await;
                let contains = inner.iter().any(|inentry| inentry.dy == entry.dy);

                if !contains {
                    inner.push(entry);
                    entries.clone().set(inner);
                }
            });
        });

        let cardselector = CardSelector::dependency_picker(f).await;

        let f = Box::new(move |card: Arc<Card>| {
            let entries = entries.clone();
            spawn(async move {
                let mut inner = entries.cloned();
                let entry = DynEntry::new(DynCard::Instances(card.id())).await;
                let contains = inner.iter().any(|inentry| inentry.dy == entry.dy);

                if !contains {
                    inner.push(entry);
                    entries.clone().set(inner);
                }
            });
        });

        let instance_selector = CardSelector::class_picker(f).await;

        Self {
            col,
            colname,
            done: Signal::new_in_scope(false, ScopeId::APP),
            instance_selector,
            entries,
            cardselector,
            dynty: Signal::new_in_scope(DynTab::Instance, ScopeId::APP),
        }
    }
}

impl Komponent for ColViewer {
    fn render(&self) -> Element {
        let mut name = self.colname.clone();
        let cards = self.entries.clone();
        let selector = self.cardselector.clone();
        let inselector = self.instance_selector.clone();
        let selv = self.clone();
        let ty = self.dynty.clone();
        let ty2 = self.dynty.clone();

        rsx! {

            div {
                h1 {"{ty2:?}"}
            }


            button {
                class: "inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base",
                onclick: move |_| {
                    let ty = ty.clone();

                    let next = match ty.cloned() {
                        DynTab::Card => DynTab::Instance,
                        DynTab::Instance => DynTab::Card,
                    };

                    ty.clone().set(next);

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
                            DynTab::Card => { selector.render() },
                            DynTab::Instance => { inselector.render() },
                        }
                    }

                }
            }


        }
    }
}

impl Overlay for ColViewer {
    fn is_done(&self) -> Signal<bool> {
        self.done.clone()
    }
}
