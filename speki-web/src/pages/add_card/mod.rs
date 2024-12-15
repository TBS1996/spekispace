use std::rc::Rc;
use std::sync::Arc;

use dioxus::prelude::*;
use frontside::{CardTy, FrontPut};
use speki_core::{AnyType, Card};
use speki_dto::CardId;
use tracing::info;

use super::add_card::backside::BackPut;
use super::CardEntry;

use crate::components::card_selector::{self, CardSelectorProps};
use crate::{App, Popup, PopupManager, Route};

pub mod backside;
mod frontside;

#[derive(Clone)]
pub struct AddCardState {
    app: App,
    front: FrontPut,
    pub back: BackPut,
    concept: Signal<Option<CardId>>,
    selected: Signal<CardTy>,
    pub searching_cards: Signal<Option<CardSelectorProps>>,
    concept_input: Signal<String>,
    concept_cards: Signal<Vec<CardEntry>>,
}

impl AddCardState {
    pub fn new(app: App) -> Self {
        let searching_cards: Signal<Option<CardSelectorProps>> = Default::default();
        let back = BackPut::new(app.clone(), searching_cards.clone());
        let front = FrontPut::new();
        let selected = front.dropdown.selected.clone();
        Self {
            app,
            front,
            back,
            concept: Default::default(),
            selected,
            searching_cards,
            concept_input: Default::default(),
            concept_cards: Default::default(),
        }
    }

    pub async fn load_cards(&self) {
        let mut concept_cards = vec![];
        let mut cards = vec![];

        for card in self.app.0.load_all_cards().await {
            if card.is_class() {
                concept_cards.push(CardEntry::new(card.clone()).await);
            }
            cards.push(CardEntry::new(card).await);
        }

        self.back.cards.clone().set(cards);
        self.concept_cards.clone().set(concept_cards);
    }

    pub fn start_concept_ref_search(&self) {
        let _selv = self.clone();

        let fun = move |card: Arc<Card<AnyType>>| {
            let selv = _selv.clone();
            spawn(async move {
                info!("setting card.. ");
                selv.set_card(card.id).await;
            });
        };

        let props = card_selector::CardSelectorProps {
            title: "choose concept card".to_string(),
            search: Signal::new_in_scope(Default::default(), ScopeId(3)),
            on_card_selected: Rc::new(fun),
            cards: self.concept_cards.clone(),
            done: Signal::new_in_scope(false, ScopeId(3)),
        };

        let popup: Popup = Box::new(props);

        let pop = use_context::<PopupManager>();
        pop.set(Route::Add {}, popup);
    }

    pub async fn set_card(&self, card: CardId) {
        info!("hey there");
        let front = self.app.0.load_card(card).await.unwrap().print().await;
        info!("2");
        self.concept_input.clone().set(front);
        self.searching_cards.clone().set(None);
    }

    fn render_norm(&self) -> Element {
        let selv = self.clone();
        let selv2 = self.clone();

        rsx! {
            div {
                style: "max-width: 500px; margin: 0 auto;",
                div {
                    h1 {
                        class: "text-2xl font-bold text-gray-800 mb-6 text-center",
                        "Add Flashcard"
                    }

                    { self.front.view() }
                    { self.back.view() }

                    match (self.selected)() {
                        CardTy::Normal => {
                            rsx ! {}
                        },
                        CardTy::Class => {
                            rsx! {
                                div {
                                    class: "block text-gray-700 text-sm font-medium mb-2",
                                    "Parent class"
                                input {
                                    class: "w-full border border-gray-300 rounded-md p-2 mb-4 text-gray-500 bg-gray-600 cursor-pointer focus:outline-none",
                                    value: "{self.concept_input}",
                                    readonly: "true",
                                    onclick: move |_| {
                                        selv2.start_concept_ref_search();
                                    },
                                }
                            }
                        }
                        },
                        CardTy::Instance => {
                            rsx! {
                                div {
                                    class: "block text-gray-700 text-sm font-medium mb-2",
                                    "Class of instance"
                                    input {
                                        class: "w-full border border-gray-300 rounded-md p-2 mb-4 text-gray-500 bg-gray-600 cursor-pointer focus:outline-none",
                                        value: "{self.concept_input}",
                                        readonly: "true",
                                        onclick: move |_| {
                                            selv2.start_concept_ref_search();
                                        },
                                    }
                                }
                            }
                        },
                    }

                    button {
                        class: "bg-blue-500 text-white py-2 px-4 rounded-md hover:bg-blue-600 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 mt-4",
                        onclick: move |_| {
                            let selv = selv.clone();
                            spawn(async move {
                                let backside = selv.back.clone();
                                let frontside = selv.front.clone();

                                let front = format!("{}", frontside.text.cloned());
                                match selv.selected.cloned() {
                                    CardTy::Normal => {
                                        let Some(back) = backside.to_backside() else {
                                            info!("oops, empty backside");
                                            return;
                                        };

                                        selv.app.0.add_card(front, back).await;

                                    },
                                    CardTy::Class => {
                                        let parent_class = selv.concept.cloned();
                                        let Some(back) = backside.to_backside() else {
                                            return;
                                        };

                                        selv.app.0.add_class(front, back, parent_class).await;


                                    },
                                    CardTy::Instance => {
                                        let Some(class) = selv.concept.cloned() else {
                                            return;
                                        };

                                        let back = backside.to_backside();
                                        selv.app.0.add_instance(front, back, class).await;
                                    },
                                }

                                frontside.reset();
                                backside.reset();
                                selv.concept.clone().set(None);

                                info!("adding new card!");
                                selv.load_cards().await;
                            });
                        },
                        "Save"
                    }
                }
            }
        }
    }

    fn render(&self) -> Element {
        self.render_norm()
    }
}

#[component]
pub fn Add() -> Element {
    let pop = use_context::<PopupManager>();
    if let Some(elm) = pop.render(Route::Add {}) {
        elm
    } else {
        use_context::<AddCardState>().render()
    }
}
