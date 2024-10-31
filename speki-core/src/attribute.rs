use crate::paths::get_attributes_path;
use crate::Card;
use crate::{get_containing_file_paths, my_sanitize_filename};
use eyre::Result;
use serde::{Deserialize, Serialize};
use speki_dto::CardId;
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug, Clone, Ord, Eq, PartialEq, PartialOrd, Copy, Hash)]
#[serde(transparent)]
pub struct AttributeId(Uuid);

impl AsRef<Uuid> for AttributeId {
    fn as_ref(&self) -> &Uuid {
        &self.0
    }
}

impl AttributeId {
    pub fn into_inner(self) -> Uuid {
        self.0
    }

    pub fn verify(id: impl AsRef<Uuid>) -> Option<Self> {
        if let Some(concept) = Attribute::load(Self(*id.as_ref())) {
            Some(concept.id)
        } else {
            None
        }
    }
}

/// An attribute of a sub-class or an instance
/// predefined questions that are valid for all in its class.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attribute {
    pub pattern: String,
    pub id: AttributeId,
    /// The attribute is valid for this class
    #[serde(alias = "concept")]
    pub class: CardId,
    // the answer to the attribute should be part of this
    // for example, if the attribute is 'where was {} born?' the type should be of concept place
    pub back_type: Option<CardId>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub dependencies: BTreeSet<CardId>,
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

    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    pub fn load_all() -> Vec<Self> {
        get_containing_file_paths(&get_attributes_path(), None)
            .into_iter()
            .map(|path| std::fs::read_to_string(path).unwrap())
            .map(|s| toml::from_str(&s).unwrap())
            .collect()
    }

    pub fn save(&self) -> Result<()> {
        let filename = my_sanitize_filename(&self.pattern);
        let path = get_attributes_path().join(filename);
        let mut f = fs::File::create(&path)?;
        let s = toml::to_string_pretty(self)?;
        f.write_all(&mut s.as_bytes())?;

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
            dependencies: Default::default(),
            back_type,
        };

        attr.save().unwrap();
        attr.id
    }
}
