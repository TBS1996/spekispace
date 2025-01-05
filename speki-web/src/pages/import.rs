use dioxus::prelude::*;

use crate::overlays::uploader::Uploader;

use crate::components::Komponent;

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
    {      state.uploader.render() }
      }
}
