use dioxus::prelude::*;
use tracing::info;

use crate::{login::LoginState, utils, Route};

pub fn tooltip_image(src: &str, msg: &str, img_size: usize, text_size: f32) -> Element {
    let size = format!("{}px", img_size.to_string());
    let text_size = format!("{}em", text_size);

    rsx! {
        div {
            class: "mr-12",

            img {
                width: "{size}",
                height: "{size}",
                src: "assets/{src}",
            }
            div {
                font_size: text_size,
                color: "white",
                "{msg}"
            }
        }
    }
}

fn route_elm(route: Route) -> Element {
    let label = route.label();
    rsx! {
        li {
            class: "mr-12",

            Link {
                class: "hover:text-gray-600",
                to: route, "{label}"
            }
        }
    }
}

pub fn nav() -> Element {
    let login = use_context::<LoginState>();
    let sig = login.inner.clone();
    rsx! {
        section { class: "relative",
            nav { class: "flex justify-between",
                div { class: "px-12 py-8 flex w-full items-center",
                    ul { class: "flex flex-row font-semibold font-heading",
                        if let Some(_) = sig.as_ref() {
                            button {
                                onclick: |_| {
                                    utils::sync_repo();
                                },
                                { tooltip_image("sync.svg", "nice", 34, 1.0) }
                            }
                        } else {
                            button {
                                onclick: |_| {
                                    info!("signing in...");
                                    spawn(async move {
                                        let mut login = use_context::<LoginState>();
                                        login.load_uncached().await;
                                    });
                                },
                                { tooltip_image("login.svg", "nice", 34, 1.0) }
                            }
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
