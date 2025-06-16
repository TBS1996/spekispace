use dioxus::prelude::*;

use crate::{Route, CURRENT_ROUTE};

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

#[cfg(not(feature = "desktop"))]
#[component]
#[cfg(not(feature = "desktop"))]
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

                        Link {
                            to: Route::Menu {  },
                            { image("burger.svg", 28, false ) }
                        }

                        { route_elm(Route::Review {}) }
                        { route_elm(Route::Add {}) }
                        { route_elm(Route::Browse {}) }
                        match LOGIN_STATE.cloned() {
                            Some(user) => {
                                    rsx!{
                                        button {
                                            onclick: move|_| {
                                                let user = user.clone();
                                                spawn(async move {
                                                    todo!()
                                                });
                                            },
                                            { image("sync.svg", 34, SYNCING.cloned()) }
                                    }
                                }
                            },
                            None => {
                                    rsx!{
                                        button {
                                            onclick: move|_| {
                                                spawn(async move {
                                                if let Some(user) = firebase::sign_in().await {
                                                    *LOGIN_STATE.write() = Some(user);
                                                }
                                                });
                                            },
                                            { image("sign_in.svg", 34, false) }
                                    }
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}

#[cfg(feature = "desktop")]
#[component]
#[cfg(feature = "desktop")]
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
                        { route_elm(Route::Review {}) }
                        { route_elm(Route::Add {}) }
                        { route_elm(Route::Browse {}) }
                    }
                }
            }
        }
    }
}
