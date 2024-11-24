use crate::card_provider::CardProvider;
use crate::App;
use speki_dto::{AttributeDTO, AttributeId, CardId};
use uuid::Uuid;

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

    pub fn from_dto(dto: AttributeDTO, card_provider: CardProvider) -> Self {
        Self {
            pattern: dto.pattern,
            id: dto.id,
            class: dto.class,
            back_type: dto.back_type,
            card_provider,
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
        let mut attrs = app.load_all_attributes(app.card_provider.clone()).await;
        attrs.retain(|attr| attr.class == class);
        attrs
    }

    pub async fn load_relevant_attributes(app: &App, card: CardId) -> Vec<Self> {
        let card = app.load_card(card).await.unwrap();
        let classes = card.load_ancestor_classes().await;
        let mut attrs = app.load_all_attributes(app.card_provider.clone()).await;
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
