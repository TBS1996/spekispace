use std::{pin::Pin, time::Duration};

use serde::{Deserialize, Serialize};
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
            card_provider: provider,
        }
    }

    pub fn into_dto(self) -> AttributeDTO {
        AttributeDTO {
            pattern: self.pattern,
            id: self.id,
            class: self.class,
            back_type: self.back_type,
        }
    }

    pub fn pattern(&self) -> &str {
        &self.pattern
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash)]
pub struct AttributeDTO {
    pub pattern: String,
    pub id: AttributeId,
    pub class: CardId,
    pub back_type: Option<CardId>,
}

pub type AttributeId = Uuid;
