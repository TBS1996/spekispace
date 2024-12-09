use std::{
    fmt::{Debug, Display},
    rc::Rc,
    sync::Arc,
};

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use speki_core::{AnyType, Card};
use speki_dto::{BackSide, CardId};
use strum::{EnumIter, IntoEnumIterator};
use tracing::info;

use super::{card_selector::CardSelectorProps, dropdown::DropDownMenu};
use crate::{components::card_selector, pages::CardEntry, utils::App};

const PLACEHOLDER: &'static str = "pick reference...";

#[derive(Clone)]
pub struct BackPut {
    text: Signal<String>,
    card: Signal<Option<CardId>>,
    show: Signal<bool>,
    dropdown: DropDownMenu<BackOpts>,
    app: App,
    ref_display: Signal<String>,
    pub searching_cards: Signal<Option<CardSelectorProps>>,
    cards: Signal<Vec<CardEntry>>,
}

impl BackPut {
    pub fn new(app: App) -> Self {
        Self {
            app,
            text: Default::default(),
            card: Default::default(),
            show: Default::default(),
            dropdown: DropDownMenu::new(BackOpts::iter()),
            ref_display: Signal::new(PLACEHOLDER.to_string()),
            searching_cards: Default::default(),
            cards: Default::default(),
        }
    }

    pub async fn load_cards(&self) {
        let mut cards = vec![];

        for card in self.app.0.load_all_cards().await {
            cards.push(CardEntry::new(card).await);
        }

        self.cards.clone().set(cards);
    }

    pub fn start_ref_search(&self) {
        let _selv = self.clone();

        let fun = move |card: Arc<Card<AnyType>>| {
            let selv = _selv.clone();
            spawn(async move {
                info!("setting card.. ");
                selv.set_card(card.id).await;
            });
        };

        let props = card_selector::CardSelectorProps {
            title: "choose reference".to_string(),
            search: Default::default(),
            on_card_selected: Rc::new(fun),
            cards: self.cards.clone(),
        };

        self.searching_cards.clone().set(Some(props));
    }

    pub async fn set_card(&self, card: CardId) {
        info!("hey there");
        let front = self.app.0.load_card(card).await.unwrap().print().await;
        info!("2");
        self.ref_display.clone().set(front);
        self.card.clone().set(Some(card));
        self.searching_cards.clone().set(None);
    }

    pub fn reset(&self) {
        self.text.clone().set(Default::default());
        self.card.clone().set(Default::default());
        self.show.clone().set(Default::default());
        self.searching_cards.clone().set(Default::default());
        self.ref_display.clone().set(PLACEHOLDER.to_string());
    }

    pub fn view(&self) -> Element {
        rsx! {
            div {
                class: "backside-editor flex items-center space-x-4",

                match *self.dropdown.selected.read() {
                    BackOpts::Text => self.render_text(),
                    BackOpts::Card => self.render_ref(),
                }

                { self.dropdown.view() }

            }
        }
    }

    pub fn to_backside(&self) -> Option<BackSide> {
        let chosen = self.dropdown.selected.cloned();
        info!("chosen is: {:?}", chosen);

        match chosen {
            BackOpts::Card => Some(BackSide::Card(self.card.cloned()?)),
            BackOpts::Text => {
                let s = self.text.cloned();
                info!("text is: {s}");

                if s.is_empty() {
                    return None;
                };

                Some(BackSide::Text(s))
            }
        }
    }

    fn render_text(&self) -> Element {
        let mut sig = self.text.clone();
        rsx! {
            input {
                class: "w-full border border-gray-300 rounded-md p-2 mb-4 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                value: "{sig}",
                oninput: move |evt| sig.set(evt.value()),
            }
        }
    }

    fn render_ref(&self) -> Element {
        info!("ref render!!");
        let card_display = self.ref_display.clone();
        let selv = self.clone();

        rsx! {
            input {
                class: "w-full border border-gray-300 rounded-md p-2 mb-4 text-gray-500 bg-gray-600 cursor-pointer focus:outline-none",
                value: "{card_display}",
                readonly: "true",
                onclick: move |_| {
                    selv.start_ref_search();
                },
            }
        }
    }
}

#[derive(Default, Copy, Clone, Debug, Serialize, Deserialize, EnumIter)]
enum BackOpts {
    #[default]
    Text,
    Card,
}

impl Display for BackOpts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            BackOpts::Text => "🔤",
            BackOpts::Card => "🔗",
        };

        write!(f, "{s}")
    }
}
