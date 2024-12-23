use dioxus::prelude::*;

use crate::Route;

pub fn image(src: &str, img_size: usize, spin: bool) -> Element {
    let size = format!("{}px", img_size);

    let class = if spin { "animate-spin" } else { "" };

    rsx! {
        div {
            class: "mr-4 flex-shrink-0 flex items-center justify-center", // Prevent shrinking and center the icon
            img {
                class: "{class}",
                style: "width: {size}; height: {size};", // Explicit size for the icon
                src: "assets/{src}",
            }
        }
    }
}

fn route_elm(route: Route) -> Element {
    let is_current = use_route::<Route>() == route;

    let classes = if is_current {
        "font-bold text-gray-950 hover:text-gray-650"
    } else {
        "text-gray-600 hover:text-gray-500"
    };

    rsx! {
        li {
            class: "mr-12",
            Link {
                class: "{classes}",
                to: route,
                aria_current: if is_current { "page" } else { "" },
                "{route.label()}"
            }
        }
    }
}

pub static SYNCING: GlobalSignal<bool> = Signal::global(|| false);

#[component]
pub fn nav() -> Element {
    rsx! {
        section { class: "relative",
            nav { class: "flex justify-between",
                div { //class: "px-12 py-8 flex w-full items-center",
                    class: "pl-12 pr-0 py-8 flex w-full items-center", // Left padding (pl-12), no right padding (pr-0)
                    ul { class: "flex flex-row font-semibold font-heading",
                        button {
                            onclick: move |_| {
                                spawn(async move{
                                    crate::utils::sync().await;
                                });
                            },
                            { image("sync.svg",  34, SYNCING.cloned()) }
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
