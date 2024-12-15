use dioxus::prelude::*;
use frontside::{CardTy, FrontPut};
use tracing::info;

use super::add_card::backside::BackPut;
use super::CardEntry;

use crate::components::cardref::CardRef;
use crate::{App, OverlayManager};

pub mod backside;
mod frontside;

#[derive(Clone)]
pub struct AddCardState {
    app: App,
    front: FrontPut,
    back: BackPut,
    concept: CardRef,
    selected: Signal<CardTy>,
    concept_cards: Signal<Vec<CardEntry>>,
}

impl AddCardState {
    pub fn new(app: App) -> Self {
        let back = BackPut::new();
        let front = FrontPut::new();
        let selected = front.dropdown.selected.clone();
        let concept_cards: Signal<Vec<CardEntry>> = Default::default();
        Self {
            app,
            front,
            concept: CardRef::new(concept_cards.clone()),
            back,
            selected,
            concept_cards,
        }
    }

    pub fn reset(&self) {
        self.front.reset();
        self.back.reset();
        self.concept.reset();
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

        self.back.ref_card.cards().set(cards);
        self.concept_cards.clone().set(concept_cards);
    }

    fn render(&self) -> Element {
        let selv = self.clone();

        rsx! {
            div {
                style: "max-width: 500px; margin: 0 auto;",
                div {
                    h1 {
                        class: "text-2xl font-bold text-gray-800 mb-6 text-center",
                        "Add Flashcard"
                    }

                    { self.front.view() }
                    { self.back.render() }

                    match (self.selected)() {
                        CardTy::Normal => {
                            rsx ! {}
                        },
                        CardTy::Class => {
                            rsx! {
                                div {
                                    class: "block text-gray-700 text-sm font-medium mb-2",
                                    "Parent class"
                                    {selv.concept.render()},
                            }
                        }
                        },
                        CardTy::Instance => {
                            rsx! {
                                div {
                                    class: "block text-gray-700 text-sm font-medium mb-2",
                                    "Class of instance"
                                    {selv.concept.render()},
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
                                        let parent_class = selv.concept.selected_card().cloned();
                                        let Some(back) = backside.to_backside() else {
                                            return;
                                        };

                                        selv.app.0.add_class(front, back, parent_class).await;


                                    },
                                    CardTy::Instance => {
                                        let Some(class) = selv.concept.selected_card().cloned() else {
                                            return;
                                        };

                                        let back = backside.to_backside();
                                        selv.app.0.add_instance(front, back, class).await;
                                    },
                                }

                                selv.reset();

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
}

#[component]
pub fn Add() -> Element {
    let pop = use_context::<OverlayManager>();
    if let Some(elm) = pop.render() {
        elm
    } else {
        use_context::<AddCardState>().render()
    }
}
