use dioxus::prelude::*;
use dioxus_elements::FileEngine;
use regex::Regex;
use std::sync::Arc;

use crate::APP;

#[derive(Eq, PartialEq, Clone)]
struct QA {
    q: String,
    a: String,
}

impl QA {
    fn extract(re: String, content: String) -> Vec<Self> {
        let Ok(re) = Regex::new(&re) else {
            return vec![];
        };

        re.captures_iter(&content)
            .map(|cap| Self {
                q: cap
                    .get(1)
                    .map_or("".to_string(), |m| m.as_str().to_string()),
                a: cap
                    .get(2)
                    .map_or("".to_string(), |m| m.as_str().to_string()),
            })
            .collect()
    }
}

#[component]
pub fn Upload() -> Element {
    let content = use_signal(String::default);
    let app = APP.cloned();
    let regex = use_signal(|| r"Q: (.*?) A: (.*)".to_string()); // Default regex pattern
    let cards = use_memo(move || QA::extract(regex.cloned(), content.cloned()));

    let read_file = move |file_engine: Arc<dyn FileEngine>, file_name: String| async move {
        if let Some(file_content) = file_engine.read_file_to_string(&file_name).await {
            content.clone().set(file_content);
        }
    };

    let upload_file = move |evt: FormEvent| async move {
        if let Some(file_engine) = evt.files() {
            if let Some(file_name) = file_engine.files().get(0) {
                read_file(file_engine, file_name.clone()).await;
            }
        }
    };

    rsx! {
        div {
            h1 { "Single File Upload Example" }

            div {
                label { r#for: "textreader", "Upload a single text file:" }
                input {
                    r#type: "file",
                    accept: "*",
                    multiple: false,
                    name: "textreader",
                    directory: false,
                    onchange: upload_file,
                }
            }

            div {
                label { r#for: "regex-input", "Regex Pattern:" }
                input {
                    r#type: "text",
                    id: "regex-input",
                    value: "{regex}",
                    oninput: move |evt| regex.clone().set(evt.value().clone()),
                }
            }

            h2 { "File Content (First 10 Lines):" }
            p {
                {content.cloned().lines().take(10).collect::<Vec<_>>().join("\n")}
            }
            button {
                onclick: move |_| {
                    let app = app.clone();
                    let cards = cards.cloned();
                    let content = content.clone();
                    spawn(async move {
                        for card in cards {
                            app.new_simple(card.q, card.a).await;
                        }

                        content.clone().set(Default::default());
                    });


                },
                "save cards"
            }

            h2 { "Extracted Cards:" }
            ul {
                for card in cards.iter() {
                    li {
                        b { "Q: " }
                        {card.q.clone()}
                        br {}
                        b { "A: " }
                        {card.a.clone()}
                    }
                }
            }
        }
    }
}
