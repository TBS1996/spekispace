use super::{
    card_selector::{CardSelector, MaybeEntry},
    itemselector::ItemSelector,
    OverlayEnum,
};
use crate::{overlays::card_selector::MyClosure, APP};
use dioxus::prelude::*;
use speki_core::{
    collection::{Collection, CollectionId, DynCard, MaybeCard, MaybeDyn},
    Card,
};
use speki_dto::Item;
use speki_web::CardEntry;
use std::{collections::BTreeMap, fmt::Display, sync::Arc};
use tracing::info;
use uuid::Uuid;

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
    dy: MaybeDyn,
    cards: Vec<MaybeCard>,
}

impl Display for DynEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let MaybeDyn::Dyn(DynCard::Card(_)) = &self.dy {
            write!(f, "{}", &self.name)
        } else {
            let qty = self.cards.len();
            write!(f, "{} ({qty})", &self.name)
        }
    }
}

async fn name(dy: MaybeDyn) -> String {
    let dy = match dy {
        MaybeDyn::Collection(id) => {
            return APP.read().load_collection(id).await.name;
        }
        MaybeDyn::Dyn(dy) => dy,
    };

    match dy {
        DynCard::Any => format!("all cards"),
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
    }
}

impl DynEntry {
    async fn new(dy: MaybeDyn) -> Self {
        let provider = APP.read().inner().card_provider();

        let name = name(dy.clone()).await;

        let cards = dy.evaluate(provider).await;

        Self { name, dy, cards }
    }
}

#[derive(Props, PartialEq, Clone)]
pub struct CollectionEditor {
    pub col: Signal<Collection>,
    pub colname: Signal<String>,
    pub done: Signal<bool>,
    pub entries: Resource<Vec<DynEntry>>,
    pub overlay: Signal<Option<OverlayEnum>>,
    pub addnew: Signal<bool>,
}

impl CollectionEditor {
    pub fn new_unsaved() -> Self {
        let col = Signal::new_in_scope(Collection::new("..".to_string()), ScopeId::APP);
        let entries = ScopeId::APP.in_runtime(move || {
            let col = col.clone();
            use_resource(move || async move {
                let mut out = vec![];
                for card in col.read().dyncards.clone().into_iter() {
                    out.push(DynEntry::new(card).await);
                }
                out
            })
        });
        Self {
            col,
            colname: Signal::new_in_scope("..".to_string(), ScopeId::APP),
            done: Signal::new_in_scope(false, ScopeId::APP),
            entries,
            overlay: Signal::new_in_scope(None, ScopeId::APP),
            addnew: Signal::new_in_scope(false, ScopeId::APP),
        }
    }

    pub fn push_entry(&mut self, card: MaybeDyn) {
        info!("pushing entry... {:?}", card);
        self.col.write().dyncards.push(card.clone());
    }

    pub fn expanded(&self) -> Resource<BTreeMap<Uuid, Signal<MaybeEntry>>> {
        info!("lets expand!");
        let selv = self.clone();
        ScopeId::APP.in_runtime(|| {
            let selv = selv.clone();
            use_resource(move || {
                let selv = selv.clone();

                async move {
                    let mut out = BTreeMap::default();
                    for card in selv
                        .col
                        .read()
                        .expand_nodeps(APP.read().inner().card_provider.clone())
                        .await
                        .into_iter()
                    {
                        let id = card.id();
                        let entry = match card {
                            MaybeCard::Id(id) => MaybeEntry::No(id),
                            MaybeCard::Card(card) => {
                                MaybeEntry::Yes(CardEntry::new(Arc::unwrap_or_clone(card)))
                            }
                        };

                        out.insert(id, Signal::new_in_scope(entry, ScopeId::APP));
                    }

                    out
                }
            })
        })
    }

    pub async fn new(id: CollectionId) -> Self {
        info!("debug 1");
        let col = APP.read().load_collection(id).await;
        let colname = Signal::new_in_scope(col.name.clone(), ScopeId::APP);
        let mut entries = vec![];
        for dy in col.dyncards.clone() {
            entries.push(DynEntry::new(dy).await);
        }

        let col = Signal::new_in_scope(col, ScopeId::APP);
        let entries = ScopeId::APP.in_runtime(move || {
            let col = col.clone();
            use_resource(move || async move {
                let mut out = vec![];
                for card in col.read().dyncards.clone().into_iter() {
                    out.push(DynEntry::new(card).await);
                }
                out
            })
        });

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
pub fn ChoiceRender(props: CollectionEditor) -> Element {
    let overlay = props.overlay.clone();
    let addnew = props.addnew;
    let col = props.col.clone();

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
                col.clone().write().insert_dyn(MaybeDyn::Dyn(DynCard::Any));
            },
            "any"
        }

        button {
            onclick: move |_|{
                overlay.clone().set(Some(OverlayEnum::CardSelector(entry_selector::card(col))));
            },
            "card"
        }
        button {
            onclick: move |_|{
                overlay.clone().set(Some(OverlayEnum::CardSelector(entry_selector::instances(col))));
            },
            "instance"
        }
        button {
            onclick: move |_|{
                overlay.clone().set(Some(OverlayEnum::CardSelector(entry_selector::dependencies(col))));
            },
            "recursive dependents"
        }
        button {
            onclick: move |_|{
                spawn(async move {
                    overlay.clone().set(Some(OverlayEnum::ColSelector(entry_selector::collection(col).await)));
                });

            },
            "collection"
        }

        }
    }
}

#[component]
pub fn ColViewRender(props: CollectionEditor) -> Element {
    let mut col = props.col.clone();
    let mut name = props.colname.clone();
    let cards = props.entries.cloned().unwrap_or_default();
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
                let mut col = selv.col.clone();
                col.write().name = name;

                spawn(async move {
                    APP.read().save_collection(col.cloned()).await;
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
                    APP.read().delete_collection(selv.col.read().id).await;
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
                    for card in cards.clone() {
                        div {
                            class: "flex flex-row mb-2",

                            button {
                                onclick: move |_| {
                                    col.write().remove_dyn(card.dy.clone());
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

    pub fn dependencies(col: Signal<Collection>) -> CardSelector {
        let f = MyClosure::new(move |card: CardEntry| {
            let mut col = col.clone();
            col.write()
                .insert_dyn(MaybeDyn::Dyn(DynCard::RecDependents(card.id())));
            async move {}
        });

        info!("debug 2");

        CardSelector::dependency_picker(f).with_title("all dependents of...".to_string())
    }

    pub fn instances(col: Signal<Collection>) -> CardSelector {
        let f = MyClosure::new(move |card: CardEntry| {
            let mut col = col.clone();
            col.write()
                .insert_dyn(MaybeDyn::Dyn(DynCard::Instances(card.id())));
            async move {}
        });

        CardSelector::class_picker(f)
    }

    pub async fn collection(col: Signal<Collection>) -> ItemSelector<Collection> {
        let mut cols = APP.read().load_collections().await;
        cols.retain(|_col| _col.id != col.read().id());
        info!("debug 4");

        let f = Box::new(move |chosen_col: Collection| {
            let mut col = col.clone();
            col.write().insert_dyn(MaybeDyn::Collection(chosen_col.id));
        });

        ItemSelector::new(cols, Arc::new(f))
    }

    pub fn card(col: Signal<Collection>) -> CardSelector {
        let f = MyClosure::new(move |card: CardEntry| {
            let mut col = col.clone();
            col.write()
                .insert_dyn(MaybeDyn::Dyn(DynCard::Card(card.id())));
            async move {}
        });

        info!("debug 3");

        CardSelector::dependency_picker(f).with_title("pick card".to_string())
    }
}
