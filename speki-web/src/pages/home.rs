use dioxus::prelude::*;

use crate::{firebase, Route};

#[component]
pub fn Menu() -> Element {
    let buttonclass = "text-center py-4 px-6 bg-blue-500 text-white font-bold rounded-lg shadow hover:bg-blue-600 transition";
    rsx! {
        div {
            class: "flex flex-col items-center justify-center min-h-screen bg-gray-50 p-6 space-y-4",

            div {
                class: "flex flex-col space-y-4 w-full max-w-md",

                Link {
                    to: Route::About {  } ,
                    class: "{buttonclass}",
                    "about"
                }
                Link {
                    to: Route::Import {  } ,
                    class: "{buttonclass}",
                    "import cards"
                }
                button {
                    class: "{buttonclass}",
                    onclick: move |_| {
                        spawn(async move {
                            firebase::sign_out().await;
                        });

                    },
                    "sign out"
                }
            }
        }
    }
}
