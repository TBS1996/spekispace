use dioxus::prelude::*;

use crate::overlays::uploader::{UploadRender, Uploader};

#[derive(Clone)]
pub struct ImportState {
    uploader: Uploader,
}

impl ImportState {
    pub fn new() -> Self {
        Self {
            uploader: Uploader::new(),
        }
    }
}

#[component]
pub fn Import() -> Element {
    let state = use_context::<ImportState>();
    rsx! {
        UploadRender {
            content: state.uploader.content.clone(),
            regex: state.uploader.regex.clone(),
            cards: state.uploader.cards.clone(),
            dropdown: state.uploader.dropdown.clone(),
            done: state.uploader.done.clone(),
            concept: state.uploader.concept.clone(),
        }
    }
}
