use std::collections::{HashMap, HashSet};

use ledgerstore::{LedgerEvent, LedgerItem};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{card::CardId, card_provider::CardProvider};

/// An attribute of a sub-class or an instance
/// predefined questions that are valid for all in its class.
#[derive(Serialize, Deserialize, Debug, Clone, Hash)]
pub struct Attribute {
    pub pattern: String,
    pub id: AttributeId,
    /// The attribute is valid for this class
    pub class: CardId,
    // the answer to the attribute should be part of this
    // for example, if the attribute is 'where was {} born?' the type should be of concept place
    pub back_type: Option<CardId>,
}

impl Attribute {
    /// Fills the pattern with the instance
    pub fn name(&self, instance: CardId, provider: CardProvider) -> String {
        let card_text = provider.load(instance).unwrap().print();

        if self.pattern.contains("{}") {
            self.pattern.replace("{}", &card_text)
        } else {
            format!("{}: {}", &self.pattern, card_text)
        }
    }

    pub fn pattern(&self) -> &str {
        &self.pattern
    }
}

pub type AttributeId = Uuid;

#[derive(Serialize, Deserialize, Clone, Debug, Hash)]
pub enum AttrAction {
    UpSert { pattern: String, class: CardId },
    SetBackType(Option<CardId>),
}

#[derive(Serialize, Deserialize, Clone, Debug, Hash)]
pub struct AttrEvent {
    id: AttributeId,
    action: AttrAction,
}

impl LedgerEvent for AttrEvent {
    type Key = AttributeId;

    fn id(&self) -> Self::Key {
        self.id
    }
}

impl LedgerItem<AttrEvent> for Attribute {
    type Error = ();
    type RefType = String;
    type PropertyType = String;

    fn run_event(mut self, event: AttrEvent) -> Result<Self, Self::Error> {
        match event.action {
            AttrAction::UpSert { pattern, class } => {
                self.pattern = pattern;
                self.class = class;
            }
            AttrAction::SetBackType(ty) => {
                self.back_type = ty;
            }
        }

        Ok(self)
    }

    fn ref_cache(&self) -> HashMap<Self::RefType, HashSet<AttributeId>> {
        todo!()
    }

    fn new_default(id: AttributeId) -> Self {
        Self {
            pattern: String::new(),
            id,
            class: Uuid::nil(),
            back_type: None,
        }
    }

    fn item_id(&self) -> AttributeId {
        self.id
    }
}

pub mod parse {
    use serde::Deserialize;
    use std::{error::Error, fs};

    use super::AttrEvent;

    #[derive(Debug, Deserialize)]
    struct RawRecords {
        records: Vec<RawContent>,
    }

    #[derive(Debug, Deserialize)]
    struct RawContent {
        content: String,
    }

    #[derive(Debug, Deserialize)]
    struct CardTemplate {
        pattern: String,
        id: String,
        class: String,
        #[serde(default)]
        back_type: Option<String>,
    }

    fn to_actions(t: CardTemplate) -> Vec<AttrEvent> {
        let id = t.id;

        let e1 = AttrEvent {
            id: id.parse().unwrap(),
            action: super::AttrAction::UpSert {
                pattern: t.pattern,
                class: t.class.parse().unwrap(),
            },
        };

        if let Some(bt) = t.back_type {
            let e2 = AttrEvent {
                id: id.parse().unwrap(),
                action: super::AttrAction::SetBackType(Some(bt.parse().unwrap())),
            };
            vec![e1, e2]
        } else {
            vec![e1]
        }
    }

    fn load_templates_from_file(path: &str) -> Result<Vec<CardTemplate>, Box<dyn Error>> {
        let json_data = fs::read_to_string(path)?;
        let raw_records: RawRecords = serde_json::from_str(&json_data)?;

        let mut templates = Vec::new();

        for raw in raw_records.records {
            match toml::from_str::<CardTemplate>(&raw.content) {
                Ok(template) => templates.push(template),
                Err(e) => eprintln!("Skipping malformed record: {e}"),
            }
        }

        Ok(templates)
    }

    pub fn load() -> Vec<AttrEvent> {
        let path = "/home/tor/Downloads/spekiattrs.json";
        let tmps = load_templates_from_file(path).unwrap();
        let mut actions: Vec<AttrEvent> = vec![];

        for acts in tmps {
            actions.extend(to_actions(acts));
        }

        actions
    }
}
