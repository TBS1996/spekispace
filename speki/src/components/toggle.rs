use dioxus::prelude::*;
use speki_core::{
    card::CardId,
    ledger::{MetaAction, MetaEvent},
};

use crate::APP;

#[component]
pub fn Toggle(
    text: &'static str,
    b: Signal<bool>,
    on_toggle: Option<EventHandler<bool>>,
) -> Element {
    rsx! {
        div {
            class: "flex items-center gap-4 mb-4",
            div {
                class: "w-24",
                p {
                    title: "card has room for improvement",
                    "{text}"
                }
            }
            div {
                class: "relative inline-block w-12 h-6 cursor-pointer",
                onclick: move |_| {
                    let new_val = !b();
                    b.set(new_val);
                    if let Some(hook) = on_toggle {
                        hook.call(new_val);
                    }
                },
                div {
                    class: "absolute top-0 left-0 w-full h-full rounded-full transition-colors duration-50 ease-in-out",
                    class: if b() {
                        "bg-blue-500"
                    } else {
                        "bg-gray-400"
                    }
                }
                div {
                    class: "absolute top-0.5 left-0.5 w-5 h-5 bg-white rounded-full shadow-md transition-transform duration-50 ease-in-out",
                    class: if b() {
                        "translate-x-6"
                    } else {
                        "translate-x-0"
                    }
                }
            }
        }
    }
}

#[component]
pub fn NeedsWork(id: CardId) -> Element {
    let signal: Signal<bool> = use_signal(|| {
        APP.read()
            .inner()
            .provider
            .metadata
            .load(id)
            .map(|meta| meta.needs_work)
            .unwrap_or_default()
    });

    rsx! {
        Toggle {
            text: "needs work",
            b: signal,
            on_toggle: Some(Callback::new(move |new_val: bool| {
                // Write update
                APP.read()
                    .inner()
                    .provider
                    .metadata
                    .modify(MetaEvent::new_modify(
                        id,
                        MetaAction::SetNeedsWork(new_val),
                    ))
                    .unwrap();

                // Re-read confirmed value
                let refreshed = APP
                    .read()
                    .inner()
                    .provider
                    .metadata
                    .load(id)
                    .map(|meta| meta.needs_work)
                    .unwrap_or_default();

                assert_eq!(refreshed, new_val);

                signal.clone().set(refreshed);
            })),
        }
    }
}
