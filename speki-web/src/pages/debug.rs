use crate::Route;

use super::*;

#[component]
pub fn Debug() -> Element {
    let state = use_context::<State>();

    let mut repopath = use_signal(|| uuid::Uuid::new_v4().simple().to_string());
    let mut remotepath = use_signal(|| REMOTE.to_string());
    let mut proxy = use_signal(|| "http://127.0.0.1:8081".to_string());
    let mut card_id = use_signal(|| "card_id".to_string());

    let mut niceinfo = state.info();
    use_effect(move || {
        log_to_console("YY");
        spawn(async move {
            let new_info = load_cached_info().await;
            log_to_console(("EYYY", &new_info));
            niceinfo.set(new_info);
        });
    });

    let flag = state.info();

    rsx! {
        Link {to: Route::Home {  }, "back home"}
        h1 {"state: {flag:?}"}
        button { onclick: move |_| {
            let state = state.clone();

            let mut info = state.info();
            spawn(async move {
                log_to_console("XX");
                let new_info = load_user_info().await;
                info.set(new_info);
            });

        }, "log in" }
        button { onclick:  |_|{
        }, "update lol" },
        button { onclick: move |_| {
            spawn(async move{
                let x = js::list_files(repopath().as_ref()).await;
                log_to_console(x);
            });
        }, "show repo!" }
        button { onclick: move |_| {
            if let Some(info) = flag.as_ref(){
                log_to_console(&info);
                js::fetch_repo(repopath().as_ref(), remotepath().as_ref(), &info.install_token, proxy().as_ref());
            } else {
                js::fetch_repo(repopath().as_ref(), remotepath().as_ref(), "foo my bar", proxy().as_ref());

            }
        }, "fetch repo!" }
        button { onclick: move |_| {
            if let Some(info) = flag.as_ref(){
                log_to_console(&info);
                js::clone_repo(repopath().as_ref(), remotepath().as_ref(), &info.install_token, proxy().as_ref());
            } else {
                js::clone_repo(repopath().as_ref(), remotepath().as_ref(), "foo my bar", proxy().as_ref());

            }
        }, "clone repo!" }

        button { onclick: move |_| {
            spawn(async move {
                for x in IndexBaseProvider::new("/foobar").load_all_attributes().await {
                    let x = format!("{:?}", x) ;
                    log_to_console(&x);
                }
            });

        }, "load cards" }
        button { onclick: move |_| {
            spawn(async move {
                if let Some(info) = flag.as_ref() {
                    log_to_console(&info);
                    let s = js::sync_repo(repopath().as_ref(), &info.auth_token, proxy().as_ref());
                    log_to_console(s);
                }
            });
        }, "sync repo" }
        button { onclick: move |_| {
            spawn(async move {
                if let Some(info) = flag.as_ref() {
                    log_to_console(&info);
                    let s = js::pull_repo(repopath().as_ref(), &info.auth_token, proxy().as_ref());
                    log_to_console(s);
                }
            });
        }, "pull repo" }
        button { onclick: move |_| {
            spawn(async move {
                if let Some(info) = flag.as_ref() {
                    let s = js::validate_upstream(repopath().as_ref(), &info.install_token);
                    log_to_console(s);
                }
            });
        }, "validate upstream" }
        button { onclick: move |_| {
            spawn(async move {
                let s = js::git_status(repopath().as_ref()).await;
                log_to_console(s);
            });
        }, "status" }
        button { onclick: move |_| {
            spawn(async move {
                let s = js::delete_dir(repopath().as_ref());
                log_to_console(s);
            });
        }, "delete dir" }
        button { onclick: move |_| {
            spawn(async move {

                let id: Uuid = card_id().parse().unwrap();
                let review = crate::pages::review::new_review(Recall::Some);
                IndexBaseProvider::new("/foobar").add_review(CardId(id), review).await;
                log_to_console("review done!");
            });
        }, "do review" }
        button { onclick: move |_| {
            spawn(async move {
                let s = js::load_filenames("/foobar/cards").await;
                log_to_console(s);
            });
        }, "debug" }
        input {
            value: "{repopath}",
            oninput: move |event| repopath.set(event.value())
        }
        input {
            value: "{remotepath}",
            oninput: move |event| remotepath.set(event.value())
        }
        input {
            value: "{proxy}",
            oninput: move |event| proxy.set(event.value())
        }
        input {
            value: "{card_id}",
            oninput: move |event| card_id.set(event.value())
        }
    }
}
