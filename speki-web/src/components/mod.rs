#[cfg(feature = "web")]
pub mod audioupload;
pub mod backside;
pub mod cardref;
pub mod dropdown;
mod filtereditor;
pub mod frontside;
pub mod graph;

pub use backside::BackPut;
pub use cardref::CardRef;
pub use dropdown::DropDownMenu;
pub use filtereditor::*;
pub use frontside::{CardTy, FrontPut};
pub use graph::GraphRep;
use speki_core::{card::CardId, Card, RefType};

use dioxus::prelude::*;
use tracing::info;

use crate::{
    overlays::{card_selector::MyClosure, cardviewer::CardViewer, OverlayEnum},
    APP,
};

#[component]
pub fn RenderDependents(
    card_id: CardId,
    overlay: Signal<Option<OverlayEnum>>,
    hidden: bool,
) -> Element {
    let show_graph = if !hidden {
        "opacity-100 visible"
    } else {
        "opacity-0 invisible"
    };

    let (deps, too_many) = {
        let mut inner = vec![];

        let dep_ids = APP
            .read()
            .inner()
            .provider
            .cards
            .get_ref_cache(RefType::Dependent, card_id);

        if dep_ids.len() > 10 {
            (vec![], true)
        } else {
            for id in dep_ids {
                let id: CardId = id.parse().unwrap();
                let card = APP.read().load_card_sync(id);
                inner.push(card);
            }
            (inner, false)
        }
    };

    rsx! {
        div {
            class: "flex flex-col {show_graph} w-full h-auto bg-white p-2 shadow-md rounded-md overflow-y-auto",


            div {
                class: "flex items-center justify-between mb-2",

                h4 {
                    class: "font-bold",
                    "Dependents"
                }

                    button {
                        class: "p-1 hover:bg-gray-200 hover:border-gray-400 border border-transparent rounded-md transition-colors",
                        onclick: move |_| {
                            spawn(async move {
                                let props = CardViewer::new().with_dependency(card_id);
                                overlay.clone().set(Some(OverlayEnum::CardViewer(props)));
                            });
                        },


                        "➕"
                    }
                }

            if too_many {
                "theres like, a lot of dependents"
            } else {
                for card in deps {
                    button {
                        class: "mb-1 p-1 bg-gray-100 rounded-md text-left",
                        onclick: move|_|{
                            let card = card.clone();
                            spawn(async move{
                                let viewer = CardViewer::new_from_card(card, Default::default()).await;
                                overlay.clone().set(Some(OverlayEnum::CardViewer(viewer)));
                            });
                        },
                        "{card}"
                    }
                }
            }
        }
    }
}
