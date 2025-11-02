use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    str::FromStr,
    sync::Arc,
};

use super::attributes::ParamAnswerEditor;
use dioxus::prelude::*;
use omtrent::TimeStamp;
use speki_core::{
    card::{AttrBackType, AttributeId, Attrv2, CardId, ParamAnswer, TextData},
    Card, CardType,
};

use crate::{
    components::{backside::BacksideError, BackPut, CardTy, DropDownMenu, FrontPut},
    overlays::{
        card_selector::MyClosure,
        cardviewer::{
            attributes::{
                load_attr_editors, load_attr_qa, load_inherited_attr_editors,
                load_inherited_param_editors, load_param_answers, AttrAnswerEditor,
                AttrBackTypeEditor, AttrEditor, AttrQandA, OldAttrAnswerEditor,
            },
            metadata::MetadataEditor,
            CardRep,
        },
    },
    pop_overlay, APP,
};

#[derive(Props, Clone, Debug)]
pub struct CardViewer {
    pub editor: CardEditor,
    pub save_hook: Option<MyClosure>,
    pub old_card: Option<Arc<Card>>,
    pub show_import: bool,
}

impl PartialEq for CardViewer {
    fn eq(&self, other: &Self) -> bool {
        self.editor == other.editor && self.old_card == other.old_card
    }
}

impl CardViewer {
    pub fn with_hook(mut self, hook: MyClosure) -> Self {
        self.save_hook = Some(hook);
        self
    }

    pub fn with_class(mut self, class: CardId) -> Self {
        self.editor.concept.clone().set(Some(class));
        self.editor.fixed_concept = true;
        self
    }

    pub fn with_front_text(self, text: String) -> Self {
        self.editor.front.text.clone().set(text);
        self
    }

    pub fn with_allowed_cards(mut self, allowed: Vec<CardTy>) -> Self {
        if allowed.is_empty() {
            return self;
        }
        self.editor.front.dropdown = DropDownMenu::new(allowed.clone(), None);
        self.editor.allowed_cards = allowed;
        self
    }

    pub fn new_from_card(card: CardId) -> Result<Self, String> {
        let mut card = match APP.read().try_load_card(card) {
            Some(card) => card,
            None => return Err("card not found".to_string()),
        };

        if card.is_attribute() {
            let instance = card.attribute_instance();
            card = match APP.read().try_load_card(instance) {
                Some(card) => card,
                None => return Err("instance of attribute is missing".to_string()),
            }
        }

        let card_id = card.id();

        let raw_ty = card.clone_base();

        let front = {
            let frnt = FrontPut::new(CardTy::from_ctype(card.card_type()));
            frnt.text.clone().set(raw_ty.data.raw_front());
            frnt
        };

        let back = BackPut::new(raw_ty.data.backside().cloned());

        let editor = {
            let concept = Signal::new_in_scope(raw_ty.data.class(), ScopeId::APP);
            let param_answers: BTreeMap<AttributeId, ParamAnswerEditor> = {
                if !card.is_instance() {
                    Default::default()
                } else {
                    load_param_answers(card.id())
                }
            };
            let params = Signal::new_in_scope(
                card.params_on_class()
                    .into_iter()
                    .map(AttrEditor::from)
                    .collect(),
                ScopeId::APP,
            );

            let attr_answers = { Signal::new_in_scope(load_attr_qa(card_id), ScopeId::APP) };

            let attrs = load_attr_editors(card.id());
            let inherited_attrs: Memo<Vec<AttrEditor>> = {
                ScopeId::APP.in_runtime(move || {
                    use_memo(move || match concept.read().as_ref() {
                        Some(class) => load_inherited_attr_editors(*class),
                        None => vec![],
                    })
                })
            };
            let inherited_params: Memo<Vec<AttrEditor>> = {
                ScopeId::APP.in_runtime(move || {
                    use_memo(move || match concept.read().as_ref() {
                        Some(class) => load_inherited_param_editors(*class, true),
                        None => vec![],
                    })
                })
            };

            let namespace = {
                if let Some(card) = card.namespace() {
                    Signal::new_in_scope(Some(card), ScopeId::APP)
                } else {
                    Signal::new_in_scope(None, ScopeId::APP)
                }
            };

            let dependencies: Signal<Vec<CardId>> = Signal::new_in_scope(
                card.explicit_dependencies().into_iter().collect(),
                ScopeId(3),
            );

            let metadata = MetadataEditor::from(
                APP.read()
                    .load_metadata(card.id())
                    .map(|m| m.clone_item())
                    .unwrap_or_default(),
            );

            CardEditor {
                front,
                attrs: Signal::new_in_scope(attrs, ScopeId::APP),
                inherited_attrs,
                param_answers: Signal::new_in_scope(param_answers, ScopeId::APP),
                inherited_params,
                params,
                attr_answers,
                namespace,
                back,
                concept,
                dependencies,
                allowed_cards: vec![],
                fixed_concept: false,
                metadata,
            }
        };

        Ok(Self {
            editor,
            old_card: Some(card.clone()),
            save_hook: None,
            show_import: false,
        })
    }

    pub fn new() -> Self {
        let concept = Signal::new_in_scope(None, ScopeId::APP);

        let inherited_attrs: Memo<Vec<AttrEditor>> = {
            ScopeId::APP.in_runtime(move || {
                use_memo(move || match concept.read().as_ref() {
                    Some(class) => load_inherited_attr_editors(*class),
                    None => vec![],
                })
            })
        };

        let inherited_params: Memo<Vec<AttrEditor>> = {
            ScopeId::APP.in_runtime(move || {
                use_memo(move || match concept.read().as_ref() {
                    Some(class) => load_inherited_param_editors(*class, true),
                    None => vec![],
                })
            })
        };

        let attr_answers: Signal<Vec<AttrQandA>> = Signal::new_in_scope(vec![], ScopeId::APP);
        let front = FrontPut::new(CardTy::Normal);
        let dependencies: Signal<Vec<CardId>> =
            Signal::new_in_scope(Default::default(), ScopeId::APP);

        let editor = {
            let back = BackPut::new(None);

            CardEditor {
                attr_answers,
                front,
                namespace: Signal::new_in_scope(None, ScopeId::APP),
                back,
                concept,
                dependencies,
                allowed_cards: vec![],
                attrs: Signal::new_in_scope(Default::default(), ScopeId::APP),
                params: Signal::new_in_scope(Default::default(), ScopeId::APP),
                param_answers: Signal::new_in_scope(Default::default(), ScopeId::APP),
                inherited_attrs,
                inherited_params,
                fixed_concept: false,
                metadata: MetadataEditor::new(),
            }
        };

        Self {
            editor,
            old_card: None,
            save_hook: None,
            show_import: false,
        }
    }

    pub fn with_dependency(mut self, dep: CardId) -> Self {
        self.editor.dependencies.push(dep);
        self
    }

    pub fn full_reset(&self) {
        if self.old_card.is_some() {
            pop_overlay();
            return;
        }

        let CardEditor {
            front,
            namespace,
            back,
            concept,
            dependencies,
            allowed_cards: _,
            attrs,
            inherited_attrs: _,
            inherited_params: _,
            attr_answers,
            fixed_concept: _,
            metadata,
            param_answers,
            params,
        } = &self.editor;

        front.reset();
        namespace.clone().set(None);
        back.reset();
        concept.clone().set(None);
        dependencies.clone().clear();
        attrs.clone().clear();
        params.clone().clear();
        param_answers.clone().write().clear();
        attr_answers.clone().clear();
        metadata.clear();
    }

    pub fn light_reset(&self) {
        self.editor.front.reset();
        self.editor.back.reset();
        self.editor.dependencies.clone().write().clear();
        self.editor.attrs.clone().write().clear();
        self.editor.params.clone().write().clear();
        self.editor.attr_answers.clone().write().clear();
    }
}

/// container for all the structs you edit while creating/modifying a card
#[derive(Props, Clone, Debug)]
pub struct CardEditor {
    pub front: FrontPut,
    pub namespace: Signal<Option<CardId>>,
    pub back: BackPut,
    pub concept: Signal<Option<CardId>>,
    pub dependencies: Signal<Vec<CardId>>,
    pub allowed_cards: Vec<CardTy>,
    pub attrs: Signal<Vec<AttrEditor>>,
    pub params: Signal<Vec<AttrEditor>>,
    pub param_answers: Signal<BTreeMap<AttributeId, ParamAnswerEditor>>,
    pub inherited_attrs: Memo<Vec<AttrEditor>>,
    pub inherited_params: Memo<Vec<AttrEditor>>,
    pub attr_answers: Signal<Vec<AttrQandA>>,
    pub fixed_concept: bool,
    pub metadata: MetadataEditor,
}

impl CardEditor {
    pub fn has_data(&self) -> bool {
        let Self {
            front,
            namespace,
            back,
            concept,
            dependencies,
            allowed_cards: _,
            attrs,
            inherited_attrs,
            inherited_params,
            attr_answers,
            fixed_concept: _,
            metadata,
            param_answers,
            params,
        } = self;

        if !params.is_empty() {
            return true;
        }

        if !param_answers.read().is_empty() {
            return true;
        }

        if !front.is_empty() {
            return true;
        }

        if !back.text.read().is_empty() || back.ref_card.card.read().is_some() {
            return true;
        }

        if namespace.read().is_some() {
            return true;
        }

        if concept.read().is_some() {
            return true;
        }

        if !dependencies.read().is_empty() {
            return true;
        }

        if !attrs.read().is_empty() {
            return true;
        }

        if !inherited_attrs.read().is_empty() {
            return true;
        }

        if !inherited_params.read().is_empty() {
            return true;
        }

        if !attr_answers.read().is_empty() {
            return true;
        }

        let MetadataEditor {
            trivial,
            suspended,
            needs_work,
        } = metadata;

        if needs_work.cloned() {
            return true;
        }

        if trivial.cloned() {
            return true;
        }

        if suspended.cloned() {
            return true;
        }

        false
    }

    pub fn into_cardrep(self) -> Result<CardRep, String> {
        let backside = self.back.clone();
        let frontside = self.front.clone();

        let front = format!("{}", frontside.text.cloned());

        if front.is_empty() {
            return Err("front side can't be empty".to_string());
        }

        for attr_qa in self.attr_answers.cloned() {
            match attr_qa {
                AttrQandA::Old {
                    answer: OldAttrAnswerEditor::TimeStamp(ts),
                    ..
                } => {
                    if let Err(_) = TimeStamp::from_str(&ts.cloned()) {
                        return Err("invalid timestamp".to_string());
                    }
                }
                AttrQandA::New { answer, .. } => {
                    if let Some(ans) = answer.cloned() {
                        if let AttrAnswerEditor::TimeStamp(ts) = ans {
                            if let Some(ts) = ts.cloned() {
                                if let Err(_) = TimeStamp::from_str(&ts) {
                                    return Err("invalid timestamp".to_string());
                                }
                            }
                        }
                    }
                }
                _ => continue,
            }
        }

        let params: BTreeMap<AttributeId, Attrv2> = self
            .params
            .cloned()
            .into_iter()
            .map(|attr| {
                (
                    attr.id,
                    Attrv2 {
                        id: attr.id,
                        pattern: attr.pattern.cloned(),
                        back_type: attr.ty.cloned().map(AttrBackType::from),
                    },
                )
            })
            .collect();

        let attrs: HashMap<AttributeId, (String, Option<AttrBackType>)> = self
            .attrs
            .cloned()
            .into_iter()
            .filter_map(|AttrEditor { id, pattern, ty }| {
                let pattern = pattern.cloned();
                let ty = match ty.cloned() {
                    Some(AttrBackTypeEditor::Timestamp) => Some(AttrBackType::TimeStamp),
                    Some(AttrBackTypeEditor::Boolean) => Some(AttrBackType::Boolean),
                    Some(AttrBackTypeEditor::InstanceOfClass(id)) => {
                        Some(AttrBackType::InstanceOfClass(id.cloned()))
                    }
                    None => None,
                };

                Some((id, (pattern, ty)))
            })
            .collect();

        let ty = match self.front.dropdown.selected.cloned() {
            CardTy::Normal => {
                let back = match backside.try_to_backside() {
                    Ok(back) => back,
                    Err(BacksideError::MissingCard) => {
                        return Err("no card selected".to_string());
                    }
                    Err(BacksideError::MissingBool) => {
                        return Err("no boolean selected".to_string());
                    }
                    Err(BacksideError::MissingText) => {
                        return Err("backside can't be empty".to_string());
                    }
                    Err(BacksideError::InvalidTimestamp) => {
                        return Err("invalid timestamp".to_string())
                    }
                };

                CardType::Normal {
                    front: TextData::from_raw(&front),
                    back,
                }
            }
            CardTy::Class => {
                let parent_class = self.concept.cloned();
                let back = match backside.try_to_backside() {
                    Ok(back) => Some(back),
                    Err(BacksideError::MissingCard) => None,
                    Err(BacksideError::MissingText) => None,
                    Err(BacksideError::MissingBool) => None,
                    Err(BacksideError::InvalidTimestamp) => {
                        return Err("invalid timestamp".to_string())
                    }
                };

                let attrs: BTreeSet<Attrv2> = attrs
                    .into_iter()
                    .map(|(id, (pattern, back_type))| Attrv2 {
                        id,
                        pattern,
                        back_type,
                    })
                    .collect();

                CardType::Class {
                    name: TextData::from_raw(&front),
                    back,
                    parent_class,
                    attrs,
                    default_question: None,
                    params,
                }
            }
            CardTy::Instance => {
                let class = match self.concept.cloned() {
                    Some(class) => class,
                    None => return Err("must pick class of instance".to_string()),
                };

                let back = match backside.try_to_backside() {
                    Ok(back) => Some(back),
                    Err(BacksideError::MissingCard) => None,
                    Err(BacksideError::MissingText) => None,
                    Err(BacksideError::MissingBool) => None,
                    Err(BacksideError::InvalidTimestamp) => {
                        return Err("invalid timestamp".to_string())
                    }
                };

                let answered_params: BTreeMap<AttributeId, ParamAnswer> = self
                    .param_answers
                    .cloned()
                    .into_iter()
                    .filter_map(|(id, ans)| {
                        if let Some(ans) =
                            ans.answer.cloned().and_then(|ans| ans.into_param_answer())
                        {
                            Some((id, ans))
                        } else {
                            None
                        }
                    })
                    .collect();

                CardType::Instance {
                    name: TextData::from_raw(&front),
                    back,
                    class,
                    answered_params,
                }
            }
            CardTy::Unfinished => CardType::Unfinished {
                front: TextData::from_raw(&front),
            },
        };

        Ok(CardRep {
            ty,
            answered_attrs: self.attr_answers.cloned(),
            namespace: self.namespace.cloned(),
            deps: self.dependencies.cloned().into_iter().collect(),
            meta: self.metadata,
        })
    }
}

impl PartialEq for CardEditor {
    fn eq(&self, other: &Self) -> bool {
        self.front == other.front
            && self.back == other.back
            && self.concept == other.concept
            && self.dependencies == other.dependencies
            && self.allowed_cards == other.allowed_cards
    }
}
