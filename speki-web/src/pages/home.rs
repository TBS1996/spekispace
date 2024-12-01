use crate::{log_to_console, login::LoginState};
use dioxus::prelude::*;
use tracing::info;

#[component]
pub fn Home() -> Element {
    use_effect(move || {
        log_to_console("YY");
        spawn(async move {
            return;
            let mut login = use_context::<LoginState>();
            if !login.load_cached().await {
                login.load_uncached().await;
            }
        });
    });

    info!("home nice");

    rsx! {
       { crate::nav::nav()  }
    }
}
