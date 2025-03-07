use dioxus::prelude::*;

use crate::Route;
#[cfg(not(feature = "desktop"))]
use crate::{firebase, LOGIN_STATE};

#[cfg(feature = "desktop")]
#[component]
#[cfg(feature = "desktop")]
pub fn Menu() -> Element {
    let buttonclass = "text-center py-4 px-6 bg-blue-500 text-white font-bold rounded-lg shadow hover:bg-blue-600 transition";
    rsx! {
        div {
            class: "flex flex-col items-center justify-center min-h-screen bg-gray-50 p-6 space-y-4",

            div {
                class: "flex flex-col space-y-4 w-full max-w-md",

                Link {
                    to: Route::Debug {  } ,
                    class: "{buttonclass}",
                    "debug"
                }
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
            }
        }
    }
}

#[cfg(not(feature = "desktop"))]
#[component]
#[cfg(not(feature = "desktop"))]
pub fn Menu() -> Element {
    let buttonclass = "text-center py-4 px-6 bg-blue-500 text-white font-bold rounded-lg shadow hover:bg-blue-600 transition";
    let loginstate = LOGIN_STATE.cloned();
    let logged_in = loginstate.is_some();

    rsx! {
        div {
            class: "flex flex-col items-center justify-center min-h-screen bg-gray-50 p-6 space-y-4",

            div {
                class: "flex flex-col space-y-4 w-full max-w-md",

                if let Some(login) = loginstate {
                    h1 {
                        class: "text-xl font-bold text-gray-800 mb-4 text-center",
                        "logged in as {login.display()}"
                    }
                } else {
                    button {
                        class: "{buttonclass}",
                        onclick: move |_| {
                            spawn(async move {
                                firebase::sign_in().await;
                            });

                        },
                        "sign in"
                    }
                }

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

                if logged_in {
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
}
