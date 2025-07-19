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
    overlays::{
        card_selector::{CardSelector, MyClosure},
        cardviewer::CardViewer,
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

    let (deps, qty): (Vec<(String, CardId)>, usize) = {
        let dep_ids = APP.read().inner().provider.cards.all_dependents(card_id);
        let qty = dep_ids.len();

        if dep_ids.len() > max_limit {
            (vec![], qty)
        } else {
            (
                dep_ids
                    .into_iter()
                    .map(|dep| {
                        (
                            APP.read()
                                .try_load_card(dep)
                                .map(|card| card.name().to_string())
                                .unwrap_or("<deleted card>".to_string()),
                            dep,
                        )
                    })
                    .collect(),
                qty,
            )
        }
    };

    let too_many = qty > max_limit;

    let children = rsx! {if too_many {
        button {
            class: "mb-1 p-1 bg-gray-100 rounded-md text-left",
            onclick: move|_|{
                let set = SetExpr::union_with(vec![DynCard::Dependents(card_id)]);
                let props = CardSelector::new(false, Default::default()).with_set(set);
                OverlayEnum::CardSelector(props).append();
            },
            "view {qty} dependents"
        }
    } else {
        for (name, id) in deps {
            button {
                class: "mb-1 p-1 bg-gray-100 rounded-md text-left",
                onclick: move|_|{
                    OverlayEnum::new_edit_card(id).append();
                },
                "{name}"
            }
        }
    }};

    rsx! {
        div {
            class: "flex flex-col {show_graph} w-full h-auto bg-white p-2 shadow-md rounded-md overflow-y-auto",

            div {
                class: "flex items-center justify-between mb-2",

                SectionWithTitle {
                    title: "Dependents".to_string(),
                    on_add: move |_| {
                        let props = CardViewer::new().with_dependency(card_id);
                        OverlayEnum::CardViewer(props).append();
                    },
                    children
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
            OverlayEnum::CardSelector(props).append();
        }
    });
}

#[component]
pub fn SectionWithTitle(
    title: String,
    on_add: Option<EventHandler<()>>,
    children: Element,
    tooltip: Option<&'static str>,
) -> Element {
    let tooltip = tooltip.unwrap_or_default();
    rsx! {
        div {
            class: "flex items-center mb-2",
            h4 {
                class: "font-bold",
                title: tooltip,
                "{title}"
            }
            if let Some(add) = on_add {
                button {
                    class: "ml-4 p-1 hover:bg-gray-200 hover:border-gray-400 border border-transparent rounded-md transition-colors",
                    onclick: move |_| add.call(()),
                    "➕"
                }
            }
        }

        div {
            class: "mt-2",
            {children}
        }
    }
}
