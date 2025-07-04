pub mod backside;
pub mod card_mastery;
pub mod cardref;
pub mod dropdown;
mod filtereditor;
pub mod frontside;

pub use backside::BackPut;
pub use cardref::CardRef;
pub use dropdown::DropDownMenu;
pub use filtereditor::*;
pub use frontside::{CardTy, FrontPut};
use speki_core::{card::CardId, collection::DynCard, set::SetExpr};

use dioxus::prelude::*;

use crate::{
    append_overlay,
    overlays::{
        card_selector::{CardSelector, MyClosure},
        cardviewer::{AdderHeader, CardViewer},
        OverlayEnum,
    },
    APP,
};

#[component]
pub fn RenderDependents(card_id: CardId, hidden: bool) -> Element {
    let show_graph = if !hidden {
        "opacity-100 visible"
    } else {
        "opacity-0 invisible"
    };

    let max_limit = 10;

    let (deps, qty) = {
        let mut inner = vec![];

        let dep_ids = APP.read().inner().provider.cards.all_dependents(card_id);

        let qty = dep_ids.len();

        if dep_ids.len() > max_limit {
            (vec![], qty)
        } else {
            for id in dep_ids {
                let card = APP.read().load_card(id);
                inner.push(card);
            }
            (inner, qty)
        }
    };

    let too_many = qty > max_limit;

    rsx! {
        div {
            class: "flex flex-col {show_graph} w-full h-auto bg-white p-2 shadow-md rounded-md overflow-y-auto",

            div {
                class: "flex items-center justify-between mb-2",

                AdderHeader {
                    title: "Dependents",
                    on_add: move |_| {
                        let props = CardViewer::new().with_dependency(card_id);
                        append_overlay(OverlayEnum::CardViewer(props));

                    },

                 }
            }

            if too_many {
                button {
                    class: "mb-1 p-1 bg-gray-100 rounded-md text-left",
                    onclick: move|_|{
                        let set = SetExpr::union_with(vec![DynCard::Dependents(card_id)]);
                        let props = CardSelector::new(false, Default::default()).with_set(set);
                        append_overlay(OverlayEnum::CardSelector(props));
                    },
                    "view {qty} dependents"
                }
            } else {
                for card in deps {
                    button {
                        class: "mb-1 p-1 bg-gray-100 rounded-md text-left",
                        onclick: move|_|{
                            let card = card.clone();
                            let viewer = CardViewer::new_from_card(card);
                            append_overlay(OverlayEnum::CardViewer(viewer));
                        },
                        "{card}"
                    }
                }
            }
        }
    }
}

pub fn set_card_link(text: Signal<String>, alias: bool) {
    let mut eval = document::eval(
        r#"
        const sel = window.getSelection();
        dioxus.send(sel ? sel.toString() : "NO_SELECTION");
    "#,
    );

    spawn(async move {
        if let Ok(val) = eval.recv::<String>().await {
            if val.len() < 2 {
                return;
            }

            let theval = val.clone();
            let f = MyClosure::new(move |card: CardId| {
                let s = if alias {
                    format!("[[{}|{}]]", card, val)
                } else {
                    format!("[[{}]]", card)
                };
                text.clone().set(text.cloned().replace(&val, &s));
            });

            let props = CardSelector::new(false, vec![])
                .new_on_card_selected(f, true)
                .with_default_search(theval)
                .with_allow_new(true);
            append_overlay(OverlayEnum::CardSelector(props));
        }
    });
}
