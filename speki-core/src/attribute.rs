use crate::Card;
use eyre::Result;
use speki_dto::{AttributeDTO, AttributeId, CardId, SpekiProvider};
use speki_fs::FileProvider;
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
}

impl Attribute {
    /// Fills the pattern with the instance
    pub fn name(&self, instance: CardId) -> String {
        let card_text = Card::from_id(instance).unwrap().print();
        if self.pattern.contains("{}") {
            self.pattern.replace("{}", &card_text)
        } else {
            format!("{}: {}", &self.pattern, card_text)
        }
    }

    fn from_dto(dto: AttributeDTO) -> Self {
        Self {
            pattern: dto.pattern,
            id: dto.id,
            class: dto.class,
            back_type: dto.back_type,
        }
    }

    fn into_dto(self) -> AttributeDTO {
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

    pub fn load_all() -> Vec<Self> {
        FileProvider::load_all_attributes()
            .into_iter()
            .map(Self::from_dto)
            .collect()
    }

    pub fn save(&self) -> Result<()> {
        FileProvider::save_attribute(self.clone().into_dto());
        Ok(())
    }

    pub fn load_from_class_only(class: CardId) -> Vec<Self> {
        let mut attrs = Self::load_all();
        attrs.retain(|attr| attr.class == class);
        attrs
    }

    pub fn load_relevant_attributes(card: CardId) -> Vec<Self> {
        let card = Card::from_id(card).unwrap();
        let classes = card.load_ancestor_classes();
        let mut attrs = Self::load_all();
        attrs.retain(|attr| classes.contains(&attr.class));
        attrs
    }

    pub fn load_from_class(class: CardId, instance: CardId) -> Vec<Self> {
        let mut attrs = Self::load_all();
        let classes = Card::from_id(instance).unwrap().load_ancestor_classes();
        attrs.retain(|attr| {
            attr.class == class
                && attr
                    .back_type
                    .map(|ty| classes.contains(&ty))
                    .unwrap_or(true)
        });
        attrs
    }

    pub fn load(id: AttributeId) -> Option<Self> {
        Self::load_all()
            .into_iter()
            .find(|concept| concept.id == id)
    }

    pub fn create(pattern: String, concept: CardId, back_type: Option<CardId>) -> AttributeId {
        let attr = Self {
            pattern,
            id: AttributeId(Uuid::new_v4()),
            class: concept,
            back_type,
        };

        attr.save().unwrap();
        attr.id
    }
}
