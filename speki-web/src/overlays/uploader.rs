use std::sync::Arc;

use dioxus::prelude::*;
use dioxus_elements::FileEngine;
use fancy_regex::Regex;
use serde::{Deserialize, Serialize};
use strum::{EnumIter, IntoEnumIterator};

use crate::{
    components::{cardref::CardRefRender, dropdown::DropComponent, CardRef, CardTy, DropDownMenu},
    overlays::OverlayEnum,
    APP,
};

#[derive(EnumIter, Clone, Serialize, Deserialize, Debug, PartialEq)]
pub enum Extraction {
    Tabs,
    Multiline,
    Semicolon,
    Custom,
}

impl std::fmt::Display for Extraction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Extraction::Tabs => "tsv",
            Extraction::Semicolon => "csv",
            Extraction::Custom => "custom",
            Extraction::Multiline => "multiline",
        };

        write!(f, "{label}")
    }
}

impl Extraction {
    fn regex(&self) -> Option<&'static str> {
        match self {
            Extraction::Tabs => Some("(.*?)\\t(.*)"),
            Extraction::Semicolon => Some("(.*?);(.*)"),
            Extraction::Multiline => Some("(?m)^(.*?)$\n^(.*?)$"),
            Extraction::Custom => None,
        }
    }
}

#[derive(Eq, PartialEq, Clone, Debug)]
pub struct QA {
    q: String,
    a: String,
}

impl QA {
    fn extract(re: String, content: String) -> Vec<Self> {
        // Compile the regex using `fancy-regex`
        let Ok(re) = Regex::new(&re) else {
            eprintln!("Invalid regex: {re:?}");
            return vec![];
        };

        // Split the content by lines and apply the regex to each line
        content
            .lines()
            .filter_map(|line| {
                // Use `re.captures` which returns `Result`
                match re.captures(line) {
                    Ok(Some(cap)) => Some(Self {
                        a: cap
                            .get(1)
                            .map_or("".to_string(), |m| m.as_str().to_string()), // Capture the answer first
                        q: cap
                            .get(2)
                            .map_or("".to_string(), |m| m.as_str().to_string()), // Capture the question second
                    }),
                    _ => None,
                }
            })
            .collect()
    }
}

#[derive(Props, PartialEq, Clone)]
pub struct Uploader {
    pub content: Signal<String>,
    pub regex: Signal<String>,
    pub cards: Signal<Vec<QA>>,
    pub dropdown: DropDownMenu<Extraction>,
    pub done: Signal<bool>,
    pub concept: CardRef,
    pub overlay: Signal<Option<OverlayEnum>>,
}

impl Uploader {
    pub fn flip_qa(&self) {
        let mut qa = self.cards.cloned();
        for x in &mut qa {
            let q = x.q.clone();
            let a = x.a.clone();

            x.q = a;
            x.a = q;
        }

        self.cards.clone().set(qa);
    }

    pub fn new() -> Self {
        let regex: Signal<String> =
            Signal::new_in_scope(Extraction::Tabs.regex().unwrap().to_string(), ScopeId(3));
        let content: Signal<String> = Signal::new_in_scope(Default::default(), ScopeId(3));
        let cards = Signal::new_in_scope(Default::default(), ScopeId(3));
        let concept = CardRef::new().with_allowed(vec![CardTy::Class]);

        let hook = move |e: Extraction| {
            if let Some(re) = e.regex() {
                regex.clone().set(re.to_string());
            }
            cards
                .clone()
                .set(QA::extract(regex.cloned(), content.cloned()));
        };

        let callback = Callback::new(hook);

        let dropdown = DropDownMenu::new(Extraction::iter(), None).with_callback(callback);

        Self {
            content,
            regex,
            cards,
            done: Signal::new_in_scope(Default::default(), ScopeId(3)),
            dropdown,
            concept,
            overlay: Default::default(),
        }
    }
}

#[component]
pub fn UploadRender(props: Uploader) -> Element {
    let content = props.content.clone();
    let app = APP.cloned();
    let regex = props.regex.clone();
    let cards = props.cards.clone();

    let read_file = move |file_engine: Arc<dyn FileEngine>, file_name: String| async move {
        if let Some(file_content) = file_engine.read_file_to_string(&file_name).await {
            content.clone().set(file_content.clone());
            cards.clone().set(QA::extract(regex.cloned(), file_content));
        }
    };

    let upload_file = move |evt: FormEvent| async move {
        if let Some(file_engine) = evt.files() {
            if let Some(file_name) = file_engine.files().get(0) {
                read_file(file_engine, file_name.clone()).await;
            }
        }
    };

    let dropdown = props.dropdown.clone();
    let concept = props.concept.clone();
    let concept2 = props.concept.clone();
    let selv = props.clone();
    let overlay = props.overlay.clone();

    rsx! {
        div {
            class: "flex flex-col gap-10",
            h1 {
                class: "text-3xl font-bold text-center",
                "Upload Cards"
            }


                    div {
                        class: "block text-gray-700 text-sm font-medium max-w-[100px] mx-auto",
                        style: "margin-right: 81px;",
                        CardRefRender{
                            selected_card: concept.card.clone(),
                            placeholder: "pick class of instance",
                            on_select: concept.on_select.clone(),
                            on_deselect: concept.on_deselect.clone(),
                            dependent: concept.dependent.clone(),
                            allowed: concept.allowed.clone(),
                            overlay: overlay.clone(),
                        },
                    }

            div {
                class: "flex flex-col md:flex-row gap-20 md:gap-5 max-h-[400px] overflow-y-auto",
                div {
                    class: "flex flex-col w-full md:w-1/3",
                    h2 {
                        class: "text-center font-bold",
                        "Content preview" }
                    div {
                        class: "border rounded p-4 bg-gray-100 text-gray-800 overflow-y-auto",
                        for line in content.cloned().split("\n") {
                            p { "{line}" }
                        }
                    }

                    div {
                        class: "mt-8",
                        label { r#for: "textreader"}
                        input {
                            r#type: "file",
                            accept: "*",
                            multiple: false,
                            name: "textreader",
                            directory: false,
                            onchange: upload_file,
                        }
                    }


                }

                div {
                    class: "flex flex-col w-full md:w-1/3",
                    label {

                        class: "text-center font-bold",
                        r#for: "regex-input", "Regex pattern:" }
                    div {
                    class: "flex flex-row",
                        input {
                            r#type: "text",
                            id: "regex-input",
                            class: "border rounded p-2",
                            value: "{regex}",
                            oninput: move |evt| {
                                regex.clone().set(evt.value().clone());
                                cards.clone().set(QA::extract(regex.cloned(), content.cloned()));
                                dropdown.set(Extraction::Custom);
                            },
                        }

                        DropComponent {options: dropdown.options.clone(), selected: dropdown.selected.clone(), hook: dropdown.hook.clone()}
                        div {
                            button {
                                onclick: move |_| {
                                    let selv = selv.clone();
                                    selv.flip_qa();
                                },
                                "flip qa"
                            }
                        }
                    }
                }

                div {
                    class: "flex flex-col w-full md:w-1/3",
                    h2 {
                        class: "text-center font-bold",
                        "Extracted cards preview:" }
                    div {
                        class: "border rounded p-4 bg-gray-100 text-gray-800 overflow-y-auto",
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

                button {
                    class: "px-4 py-2 bg-blue-500 text-white rounded hover:bg-blue-600",
                    onclick: move |_| {
                        let app = app.clone();
                        let entries = cards.cloned();
                        let content = content.clone();
                        let concept = concept2.clone();
                        spawn(async move {

                            match concept.selected_card().cloned() {
                                Some(class) => {
                                    for card in entries {
                                        app.new_instance(card.q, Some(card.a), class).await;
                                    }
                                },
                                None => {
                                    for card in entries {
                                        app.new_simple(card.q, card.a).await;
                                    }
                                },
                            }


                            content.clone().set(Default::default());
                            cards.clone().set(Default::default());
                        });
                    },
                    "Save Cards"
                }
                }
            }
        }
    }
}
