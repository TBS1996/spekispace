use dioxus::prelude::*;

use crate::{Route, CURRENT_ROUTE};

pub fn image(src: &str, img_size: usize, spin: bool) -> Element {
    let size = format!("{}px", img_size);

    let class = if spin { "animate-spin" } else { "" };

    rsx! {
        div {
            class: "mr-4 flex-shrink-0 flex items-center justify-center",
            img {
                class: "{class}",
                style: "width: {size}; height: {size};",
                src: "assets/{src}",
            }
        }
    }
}

pub static SYNCING: GlobalSignal<bool> = Signal::global(|| false);

fn route_elm(route: Route) -> Element {
    let is_current = CURRENT_ROUTE.cloned() == route;

    let classes = if is_current {
        "font-bold text-gray-950 hover:text-gray-650 text-lg"
    } else {
        "text-gray-600 hover:text-gray-500 text-lg"
    };

    rsx! {
        li {
            class: "mr-8",
            Link {
                class: "{classes}",
                to: route,
                aria_current: if is_current { "page" } else { "" },
                "{route.label()}"
            }
        }
    }
}

#[component]
pub fn nav() -> Element {
    rsx! {
        section {
            class: "relative w-full",
            nav {
                class: "flex justify-between items-center w-full p-0 overflow-hidden",
                div {
                    class: "flex w-full items-center lg:pl-12 lg:py-8 pl-4 py-4 flex-nowrap",
                    ul {
                        class: "flex flex-row font-semibold font-heading space-x-6",
                        button {
                            onclick: move |_| {
                                spawn(async move {
                                    crate::utils::sync().await;
                                });
                            },
                            { image("sync.svg", 34, SYNCING.cloned()) }
                        }
                        { route_elm(Route::Review {}) }
                        { route_elm(Route::Add {}) }
                        { route_elm(Route::Browse {}) }
                    }
                }
            }
        }
    }
}
