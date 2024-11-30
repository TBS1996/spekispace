use dioxus::prelude::*;

use crate::App;

#[component]
pub fn Add() -> Element {
    let mut frontside = use_signal(|| "".to_string());
    let mut backside = use_signal(|| "".to_string());
    rsx! {
        crate::nav::nav{}

        div {
            style: "max-width: 500px; margin: 0 auto;",

            div {
                h1 {
                    class: "text-2xl font-bold text-gray-800 mb-6 text-center",
                    "Add Flashcard"
                }

                label {
                    class: "block text-gray-700 text-sm font-medium mb-2",
                    "Front:"
                    input {
                        class: "w-full border border-gray-300 rounded-md p-2 mb-4 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                        value: "{frontside}",
                        oninput: move |evt| frontside.set(evt.value()),
                    }
                }

                label {
                    class: "block text-gray-700 text-sm font-medium mb-2",
                    "Back:"
                    input {
                        class: "w-full border border-gray-300 rounded-md p-2 mb-4 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                        value: "{backside}",
                        oninput: move |evt| backside.set(evt.value()),
                    }
                }

                button {
                    class: "bg-blue-500 text-white py-2 px-4 rounded-md hover:bg-blue-600 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 mt-4",
                    onclick: move |_| {
                        spawn(async move {
                            let front = format!("{frontside}");
                            let back = format!("{backside}");
                            use_context::<App>().0.add_card(front, back).await;
                            frontside.set(String::new());
                            backside.set(String::new());
                        });
                    },
                    "Save"
                }
            }
        }
    }
}
