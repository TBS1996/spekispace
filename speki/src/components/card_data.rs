use dioxus::prelude::*;
use speki_core::card::{CardId, EvalText};

use crate::APP;

enum Data {
    Normal,
    Class {
        parent_class: Option<(String, CardId)>,
    },
    Instance {
        class: (String, CardId),
        params: Vec<(String, String)>,
    },
    Attribute {
        instance: (String, CardId),
    },
    Statement,
    Unfinished,
    Event,
    Invalid,
}

impl Data {
    fn new(id: CardId) -> Self {
        let card = APP.read().try_load_card(id).unwrap();
        use speki_core::CardType;
        let card_provider = APP.read().inner().card_provider.clone();

        match &card.clone_base().data {
            CardType::Instance {
                class: class_id,
                answered_params: _,
                ..
            } => {
                let class_name = APP
                    .read()
                    .try_load_card(*class_id)
                    .unwrap()
                    .name()
                    .to_string();
                let mut params: Vec<(String, String)> = vec![];

                for (attr, ans) in card.param_to_ans() {
                    if let Some(ans) = ans {
                        let back =
                            EvalText::from_backside(&ans.answer, &card_provider, false, true)
                                .to_string();

                        params.push((attr.pattern, back));
                    }
                }

                params.sort();

                Data::Instance {
                    class: (class_name, *class_id),
                    params,
                }
            }
            CardType::Normal { .. } => Data::Normal,
            CardType::Unfinished { .. } => Data::Unfinished,
            CardType::Attribute { instance, .. } => match APP.read().card_name(*instance) {
                Some(instance_name) => Data::Attribute {
                    instance: (instance_name.to_string(), *instance),
                },
                None => Data::Invalid,
            },
            CardType::Class { parent_class, .. } => match parent_class {
                Some(parent) => match APP.read().card_name(*parent) {
                    Some(parent_class_name) => Data::Class {
                        parent_class: Some((parent_class_name.to_string(), *parent)),
                    },
                    None => Data::Invalid,
                },
                None => Data::Class { parent_class: None },
            },
            CardType::Statement { .. } => Data::Statement,
            CardType::Event { .. } => Data::Event,
        }
    }
}

#[component]
pub fn CardData(id: CardId) -> Element {
    let data = Data::new(id);

    rsx! {
        div {
            class: "card-data p-2 rounded bg-gray-100 text-sm space-y-1",

            match data {
                Data::Normal => rsx!(div { "Type: Normal" }),
                Data::Unfinished => rsx!(div { "Type: Unfinished" }),
                Data::Statement => rsx!(div { "Type: Statement" }),
                Data::Event => rsx!(div { "Type: Event" }),
                Data::Invalid => rsx!(div { "Type: Invalid (broken link)" }),

                Data::Class { parent_class } => rsx!(
                    div { "Type: Class" }
                    match parent_class {
                        Some((name, _)) => rsx!(div { "Parent class: {name}" }),
                        None => rsx!(div { "No parent class" }),
                    }
                ),

                Data::Attribute { instance: (name, _) } => rsx!(
                    div { "Type: Attribute" }
                    div { "Instance: {name}" }
                ),

                Data::Instance { class: (class_name, _), params } => rsx!(
                    div { "Type: Instance" }
                    div { "Class: {class_name}" }

                    if !params.is_empty() {
                        div {
                            "Parameters:"
                            for (attr, val) in params {
                                ul {
                                    class: "list-disc list-inside",
                                        li { strong { "{attr}: " } "{val}" }

                                }

                            }
                        }
                    }
                ),
            }
        }
    }
}
