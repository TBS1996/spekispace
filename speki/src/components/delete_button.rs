use dioxus::prelude::*;
use ledgerstore::LedgerEvent;
use speki_core::{card::CardId, collection::DynCard, set::SetExpr};

use crate::{
    overlays::{
        card_selector::{CardSelector, MyClosure},
        OverlayEnum,
    },
    pop_overlay,
    utils::handle_card_event_error,
    APP,
};

#[component]
pub fn DeleteButton(
    card_id: CardId,
    pop_ol: Option<bool>,
    f: Option<MyClosure>,
    class: Option<&'static str>,
    #[props(default = false)] show_deps: bool,
    #[props(default = false)] disabled: bool,
) -> Element {
    let card = APP.read().load(card_id);
    //debug_assert!(card.is_some());

    let dependents = card.map(|c| c.dependents_ids());

    let title: Option<&'static str> = match dependents {
        Some(deps) => {
            if deps.is_empty() {
                None
            } else {
                Some("cannot delete card with dependents")
            }
        }
        None => Some("missing card"),
    };

    let has_deps = title.is_some();
    let title = title.unwrap_or_default();
    let pop_ol = pop_ol.unwrap_or(true);
    let disabled = has_deps && !show_deps && !disabled;

    let class = format!(
        "{} {}",
        crate::styles::DELETE_BUTTON,
        class.unwrap_or_default()
    );

    rsx! {
        button {
            class,
            title: "{title}",
            disabled,
            onclick: move |_| {
                let Some(has_deps) = APP.read().load(card_id).map(|c|!c.dependents_ids().is_empty()) else {
                    return;
                };

                if has_deps {
                    let set = SetExpr::union_with(vec![DynCard::Dependents(card_id)]);
                    let props = CardSelector::new(true, Default::default()).with_set(set);
                    OverlayEnum::CardSelector(props).append();
                    OverlayEnum::new_notice("cannot delete card due to dependents").append();
                } else {
                    if let Err(e) = APP.read().modify_card(LedgerEvent::new_delete(card_id)) {
                        handle_card_event_error(e);
                        return;
                    }
                    if let Some(f) = &f {
                        f.call(card_id);
                    }

                    if pop_ol {
                        pop_overlay();
                    }
                }
            },
            "delete"
        }
    }
}
