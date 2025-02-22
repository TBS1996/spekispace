use std::{pin::Pin, time::Duration};

use serde::{Deserialize, Serialize};
use speki_dto::{Item, ModifiedSource};
use uuid::Uuid;

use crate::{card::CardId, card_provider::CardProvider, App};

/// An attribute of a sub-class or an instance
/// predefined questions that are valid for all in its class.
#[derive(Debug, Clone)]
pub struct Attribute {
    pub pattern: String,
    pub id: AttributeId,
    /// The attribute is valid for this class
    pub class: CardId,
    // the answer to the attribute should be part of this
    // for example, if the attribute is 'where was {} born?' the type should be of concept place
    pub back_type: Option<CardId>,
    pub card_provider: CardProvider,
    pub last_modified: Duration,
    pub source: ModifiedSource,
}

impl Attribute {
    /// Fills the pattern with the instance
    pub fn name(
        &self,
        instance: CardId,
    ) -> Pin<Box<dyn std::future::Future<Output = Option<String>> + '_>> {
        Box::pin(async move {
            let card_text = self.card_provider.load(instance).await?.print().await;

            Some(if self.pattern.contains("{}") {
                self.pattern.replace("{}", &card_text)
            } else {
                format!("{}: {}", &self.pattern, card_text)
            })
        })
    }

    pub fn from_dto(dto: AttributeDTO, provider: CardProvider) -> Self {
        Self {
            pattern: dto.pattern,
            id: dto.id,
            class: dto.class,
            back_type: dto.back_type,
            last_modified: dto.last_modified,
            source: dto.source,
            card_provider: provider,
        }
    }

    pub fn into_dto(self) -> AttributeDTO {
        AttributeDTO {
            pattern: self.pattern,
            id: self.id,
            class: self.class,
            back_type: self.back_type,
            last_modified: self.last_modified,
            deleted: false,
            source: self.source,
        }
    }

    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    pub async fn load_from_class_only(app: &App, class: CardId) -> Vec<Self> {
        let mut attrs: Vec<Attribute> = app
            .provider
            .attrs
            .load_all()
            .await
            .into_values()
            .map(|a| Self::from_dto(a, app.card_provider.clone()))
            .collect();
        attrs.retain(|attr| attr.class == class);
        attrs
    }

    /*

    pub fn load_from_class(class: CardId, instance: CardId, app: &App) -> Vec<Self> {
        let mut attrs = Self::load_all();
        let classes = app
            .provider
            .load_card(instance)
            .map(|raw| Card::from_raw(app, raw))
            .unwrap()
            .load_ancestor_classes(app);
        attrs.retain(|attr| {
            attr.class == class
                && attr
                    .back_type
                    .map(|ty| classes.contains(&ty))
                    .unwrap_or(true)
        });
        attrs
    }

    */
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash)]
pub struct AttributeDTO {
    pub pattern: String,
    pub id: AttributeId,
    pub class: CardId,
    pub back_type: Option<CardId>,
    #[serde(default)]
    pub last_modified: Duration,
    #[serde(default)]
    pub deleted: bool,
    #[serde(default)]
    pub source: ModifiedSource,
}

pub type AttributeId = Uuid;

impl Item for AttributeDTO {
    type PreviousVersion = Self;
    type Key = AttributeId;

    fn last_modified(&self) -> Duration {
        self.last_modified
    }

    fn set_last_modified(&mut self, time: Duration) {
        self.last_modified = time;
    }

    fn set_source(&mut self, source: ModifiedSource) {
        self.source = source;
    }

    fn source(&self) -> ModifiedSource {
        self.source
    }

    fn id(&self) -> Uuid {
        self.id
    }

    fn identifier() -> &'static str {
        "attributes"
    }

    fn deleted(&self) -> bool {
        self.deleted
    }

    fn set_delete(&mut self) {
        self.deleted = true;
    }
}
