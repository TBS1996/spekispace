use dioxus::prelude::*;
use speki_core::metadata::Metadata;

use crate::components::{SectionWithTitle, Toggle};

#[derive(Debug, Clone, PartialEq)]
pub struct MetadataEditor {
    pub suspended: Signal<bool>,
    pub needs_work: Signal<bool>,
    pub trivial: Signal<bool>,
}

impl MetadataEditor {
    pub fn new() -> Self {
        Self {
            suspended: Signal::new_in_scope(false, ScopeId::APP),
            trivial: Signal::new_in_scope(false, ScopeId::APP),
            needs_work: Signal::new_in_scope(false, ScopeId::APP),
        }
    }

    pub fn clear(&self) {
        let Self {
            suspended,
            trivial,
            needs_work,
        } = Self::new();
        self.suspended.clone().set(suspended.cloned());
        self.trivial.clone().set(trivial.cloned());
        self.needs_work.clone().set(needs_work.cloned());
    }
}

impl From<Metadata> for MetadataEditor {
    fn from(value: Metadata) -> Self {
        let Metadata {
            trivial,
            suspended,
            id: _,
            needs_work,
        } = value;

        Self {
            suspended: Signal::new_in_scope(suspended.is_suspended(), ScopeId::APP),
            trivial: Signal::new_in_scope(trivial.unwrap_or_default(), ScopeId::APP),
            needs_work: Signal::new_in_scope(needs_work, ScopeId::APP),
        }
    }
}

#[component]
pub fn DisplayMetadata(metadata: MetadataEditor) -> Element {
    let MetadataEditor {
        suspended,
        trivial: _,
        needs_work,
    } = metadata;

    rsx! {
        SectionWithTitle {
            title: "Metadata".to_string(),
            children: rsx! {
                //Toggle { text: "trivial", b: trivial  }
                Toggle { text: "suspended", b: suspended  }
                Toggle { text: "needs work", b: needs_work  }
            },
        }
    }
}
