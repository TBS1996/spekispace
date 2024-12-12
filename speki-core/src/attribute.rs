use std::time::Duration;

use crate::App;
use crate::{card_provider::CardProvider, Provider};
use speki_dto::{AttributeDTO, AttributeId, CardId};
use uuid::Uuid;

pub struct AttrProvider {
    provider: Provider,
    card_provider: CardProvider,
}

impl AttrProvider {
    pub fn new(provider: Provider, card_provider: CardProvider) -> Self {
        Self {
            provider,
            card_provider,
        }
    }

    pub async fn load_all(&self) -> Vec<Attribute> {
        let mut out = vec![];

        for attr in self.provider.load_all_attributes().await {
            let modified = self.provider.last_modified_attribute(attr.id).await;
            let attr = Attribute::from_dto(attr, self.card_provider.clone(), modified);
            out.push(attr);
        }

        out
    }

    pub async fn save(&self, attribute: Attribute) {
        self.provider
            .save_attribute(Attribute::into_dto(attribute))
            .await;
    }

    pub async fn load(&self, id: AttributeId) -> Option<Attribute> {
        let last_modified = self.provider.last_modified_attribute(id).await;
        let card_provider = self.card_provider.clone();
        self.provider
            .load_attribute(id)
            .await
            .map(|dto| Attribute::from_dto(dto, card_provider.clone(), last_modified))
    }

    pub async fn delete(&self, id: AttributeId) {
        self.provider.delete_attribute(id).await;
    }
}

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
}

impl Attribute {
    /// Fills the pattern with the instance
    pub async fn name(&self, instance: CardId) -> String {
        let card_text = self
            .card_provider
            .load(instance)
            .await
            .unwrap()
            .print()
            .await;

        if self.pattern.contains("{}") {
            self.pattern.replace("{}", &card_text)
        } else {
            format!("{}: {}", &self.pattern, card_text)
        }
    }

    pub fn from_dto(
        dto: AttributeDTO,
        card_provider: CardProvider,
        last_modified: Duration,
    ) -> Self {
        Self {
            pattern: dto.pattern,
            id: dto.id,
            class: dto.class,
            back_type: dto.back_type,
            card_provider,
            last_modified,
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

    pub async fn load_from_class_only(app: &App, class: CardId) -> Vec<Self> {
        let mut attrs = app.load_all_attributes().await;
        attrs.retain(|attr| attr.class == class);
        attrs
    }

    pub async fn load_relevant_attributes(app: &App, card: CardId) -> Vec<Self> {
        let card = app.load_card(card).await.unwrap();
        let classes = card.load_ancestor_classes().await;
        let mut attrs = app.load_all_attributes().await;
        attrs.retain(|attr| classes.contains(&attr.class));
        attrs
    }

    pub async fn create(
        app: &App,
        pattern: String,
        concept: CardId,
        back_type: Option<CardId>,
    ) -> AttributeId {
        let x = Self {
            pattern,
            id: AttributeId(Uuid::new_v4()),
            class: concept,
            back_type,
            card_provider: app.card_provider.clone(),
            last_modified: Duration::default(),
        };
        let id = x.id;
        app.save_attribute(x).await;
        id
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
