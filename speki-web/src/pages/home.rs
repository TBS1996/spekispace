use super::*;

use crate::Route;

#[component]
pub fn Home() -> Element {
    rsx! {
        div {
            display: "flex",
            flex_direction: "column",
            Link { to: Route::Review {}, "lets review!" }
            Link { to: Route::Add {}, "add cards" }
            Link { to: Route::Debug {}, "debug" }
        }
    }
}
