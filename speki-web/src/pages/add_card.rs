use crate::Route;

use super::*;

#[component]
pub fn Add() -> Element {
    let mut frontside = use_signal(|| "".to_string());
    let mut backside = use_signal(|| "".to_string());
    rsx! {
            div {
                style: "padding: 20px; max-width: 600px; margin: 0 auto; font-family: Arial;",
                Link {to: Route::Home {  }, "back home"}
                h1 { "Add a Flashcard" }
                label {
                    "Frontside:"
                    input {
                        style: "width: 100%; margin-bottom: 10px; padding: 5px;",
                        value: "{frontside}",
                        oninput: move |evt| frontside.set(evt.value()),
                    }
                }
                br {}
                label {
                    "Backside:"
                    input {
                        style: "width: 100%; margin-bottom: 10px; padding: 5px;",
                        value: "{backside}",
                        oninput: move |evt| backside.set(evt.value()),
                    }
                }
                br {}
                button {
                    style: "padding: 10px; background: lightblue; border: 1px solid gray; cursor: pointer;",
                    onclick: move |_| {
                        spawn(async move {
                            let front = format!("{frontside}");
                            let back = format!("{backside}");
                            use_context::<State>().app.add_card(front, back).await;
                            frontside.set(String::new());
                            backside.set(String::new());
                        });
                    },
                    "Save"
                }
            }


    }
}
