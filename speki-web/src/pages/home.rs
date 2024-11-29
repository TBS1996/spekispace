use crate::{js, load_cached_info, log_to_console, Route, State, PROXY, REPO_PATH};
use dioxus::prelude::*;

#[component]
pub fn Home() -> Element {
    let state = use_context::<State>();
    let mut username = state.username();
    let mut info = state.info();

    use_effect(move || {
        log_to_console("YY");
        spawn(async move {
            let new_info = load_cached_info().await;
            if let Some(info) = &new_info {
                username.set(info.username.cloned());
            }
            log_to_console(("EYYY", &new_info));
            info.set(new_info);
        });
    });

    let flag = info.as_ref().is_some();

    rsx! {
        div {
            display: "flex",
            flex_direction: "column",

            if flag {
                div {
                    display: "flex",
                    flex_direction: "row",
                    h1 { "logged in as {username}" }
                    button {
                        width: "200px",
                        height: "30",
                        onclick: move |_| {
                        spawn(async move {
                            let Some(info) = info.as_ref() else {
                                return;
                            };

                            log_to_console(&info);
                            let s = js::sync_repo(REPO_PATH, &info.auth_token, PROXY);
                            log_to_console(s);
                        });
                    }, "sync" }
                }
            } else {
                button {
                    width: "200px",
                    onclick: move |_| {
                    spawn(async move {
                        let state = use_context::<State>();
                        state.load_user_info().await;
                    });

                }, "log in" }
            }


            Link { to: Route::Review {}, "lets review!" }
            Link { to: Route::Add {}, "add cards" }
            Link { to: Route::Debug {}, "debug" }
        }
    }
}
