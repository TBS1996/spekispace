use dioxus::prelude::*;
use speki_dto::CardId;
use tracing::info;

use crate::components::BackPut;
use crate::components::CardRef;
use crate::components::CardTy;
use crate::components::FrontPut;
use crate::utils::CardEntries;
use crate::App;
use crate::Komponent;

#[derive(Clone)]
pub struct AddCardState {
    app: App,
    front: FrontPut,
    back: BackPut,
    concept: CardRef,
    selected: Signal<CardTy>,
}

impl AddCardState {
    pub fn new(app: App, entries: CardEntries) -> Self {
        let back = BackPut::new();
        let front = FrontPut::new();
        let selected = front.dropdown.selected.clone();
        Self {
            app,
            front,
            concept: CardRef::new(entries.classes.clone()),
            back,
            selected,
        }
    }

    pub fn reset(&self) {
        self.front.reset();
        self.back.reset();
        self.concept.reset();
    }

    async fn add_card(&self) -> Option<CardId> {
        let backside = self.back.clone();
        let frontside = self.front.clone();

        let front = format!("{}", frontside.text.cloned());
        let id = match self.selected.cloned() {
            CardTy::Normal => {
                let back = backside.to_backside()?;

                self.app.0.add_card(front, back).await
            }
            CardTy::Class => {
                let parent_class = self.concept.selected_card().cloned();
                let back = backside.to_backside()?;

                self.app.0.add_class(front, back, parent_class).await
            }
            CardTy::Instance => {
                let class = self.concept.selected_card().cloned()?;

                let back = backside.to_backside();
                self.app.0.add_instance(front, back, class).await
            }
            CardTy::Unfinished => self.app.0.add_unfinished(front).await,
        };

        Some(id)
    }
}

#[component]
pub fn Add() -> Element {
    let selv = use_context::<AddCardState>();

    rsx! {
        div {
            style: "max-width: 500px; margin: 0 auto;",
            div {
                h1 {
                    class: "text-2xl font-bold text-gray-800 mb-6 text-center",
                    "Add Flashcard"
                }

                { selv.front.render() }


                match (selv.selected)() {
                    CardTy::Unfinished => {
                        rsx! {}
                    }
                    CardTy::Normal => {
                        rsx! {
                            { selv.back.render() }
                        }
                    }
                    CardTy::Class => {
                        rsx! {
                            { selv.back.render() }
                                div {
                                    class: "block text-gray-700 text-sm font-medium mb-2",
                                    "Parent class"
                                    {selv.concept.render()},
                            }
                        }
                    }
                    CardTy::Instance => {
                        rsx! {
                            { selv.back.render() }

                            div {
                                class: "block text-gray-700 text-sm font-medium mb-2",
                                "Class of instance"
                                {selv.concept.render()},
                            }
                        }
                    }
                }

                button {
                    class: "bg-blue-500 text-white py-2 px-4 rounded-md hover:bg-blue-600 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 mt-4",
                    onclick: move |_| {
                        let selv = selv.clone();
                        spawn(async move {
                            if selv.add_card().await.is_some(){
                                selv.reset();
                                info!("adding new card!");
                            };
                        });
                        },
                    "Save"
                }
            }
        }
    }
}
