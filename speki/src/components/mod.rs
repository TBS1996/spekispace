pub mod backside;
pub mod card_mastery;
pub mod cardref;
mod delete_button;
pub mod dropdown;
mod filtereditor;
pub mod frontside;
pub mod linkeditor;
mod toggle;

pub use backside::BackPut;
pub use cardref::CardRef;
pub use delete_button::DeleteButton;
pub use dropdown::DropDownMenu;
pub use filtereditor::*;
pub use frontside::{CardTy, FrontPut};
pub use linkeditor::LinkEditor;
use speki_core::{
    card::{normalize_string, CardId},
    collection::DynCard,
    set::SetExpr,
    CardRefType,
};
pub use toggle::Toggle;

use dioxus::prelude::*;
use serde::Deserialize;

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

    let (deps, qty): (Vec<(String, CardRefType, CardId)>, usize) = {
        let dep_ids = APP.read().all_dependents_with_ty(card_id);
        let qty = dep_ids.len();

        if dep_ids.len() > max_limit {
            (vec![], qty)
        } else {
            (
                dep_ids
                    .into_iter()
                    .map(|(ty, dep)| {
                        (
                            APP.read()
                                .try_load_card(dep)
                                .map(|card| card.front_side().to_string())
                                .unwrap_or("<deleted card>".to_string()),
                            ty,
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
        for (name, ty, id) in deps {
            button {
                class: "mb-1 p-1 bg-gray-100 rounded-md text-left",
                title: ty.to_str(),
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

pub fn set_card_link(text: Signal<String>, alias: bool, direct_match: bool) {
    #[derive(Deserialize)]
    struct SelectionData {
        before: Option<String>,
        selected: String,
        after: Option<String>,
    }

    let mut eval = document::eval(
        r#"
        const active = document.activeElement;
        if (active && (active.tagName === "INPUT" || active.tagName === "TEXTAREA")) {
            const value = active.value ?? "";
            const start = active.selectionStart ?? 0;
            const end = active.selectionEnd ?? start;
            const lo = Math.min(start, end);
            const hi = Math.max(start, end);
            dioxus.send({
                before: value.slice(0, lo),
                selected: value.slice(lo, hi),
                after: value.slice(hi),
            });
        } else {
            const sel = window.getSelection();
            dioxus.send({
                before: null,
                selected: sel ? sel.toString() : "",
                after: null,
            });
        }
    "#,
    );

    spawn(async move {
        if let Ok(selection) = eval.recv::<SelectionData>().await {
            if selection.selected.len() < 2 {
                return;
            }

            let selected = selection.selected.clone();
            let before = selection.before.clone();
            let after = selection.after.clone();
            let value = selected.clone();
            let f = MyClosure::new(move |card: CardId| {
                let s = if alias {
                    format!("[[{}|{}]]", card, value.clone())
                } else {
                    format!("[[{}]]", card)
                };
                if let (Some(before), Some(after)) = (before.as_ref(), after.as_ref()) {
                    text.clone().set(format!("{before}{s}{after}"));
                } else {
                    text.clone().set(text.cloned().replacen(&value, &s, 1));
                }
            });

            if direct_match {
                let app = APP.read().clone();
                if let Some(card) = unique_direct_match(&app, &selected) {
                    f.call(card);
                    return;
                }
            }

            let props = CardSelector::new(false, vec![])
                .new_on_card_selected(f, true)
                .with_default_search(selected)
                .with_allow_new(true);
            OverlayEnum::CardSelector(props).append();
        }
    });
}

fn unique_direct_match(app: &crate::utils::App, selected: &str) -> Option<CardId> {
    let selected = selected.trim();
    if selected.is_empty() {
        return None;
    }

    let lowered = selected.to_lowercase();
    let normalized = normalize_string(selected);
    let card_ids = app.eval_expr(&SetExpr::All);

    let mut exact = Vec::new();
    let mut case_insensitive = Vec::new();
    let mut normalized_matches = Vec::new();

    for card_id in card_ids {
        let Some(card) = app.try_load_card(card_id) else {
            continue;
        };
        let name = card.name().to_string();

        if name == selected {
            exact.push(card_id);
        } else if name.to_lowercase() == lowered {
            case_insensitive.push(card_id);
        } else if normalize_string(&name) == normalized {
            normalized_matches.push(card_id);
        }
    }

    if exact.len() == 1 {
        return exact.into_iter().next();
    }
    if !exact.is_empty() {
        return None;
    }

    if case_insensitive.len() == 1 {
        return case_insensitive.into_iter().next();
    }
    if !case_insensitive.is_empty() {
        return None;
    }

    if normalized_matches.len() == 1 {
        return normalized_matches.into_iter().next();
    }

    None
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
            class: "flex flex-col",

            div {
                class: "flex items-center mb-1",
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
}
