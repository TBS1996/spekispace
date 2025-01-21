use crate::components::Komponent;
use dioxus::prelude::*;

pub mod card_selector;
pub mod cardviewer;
pub mod colviewer;
pub mod itemselector;
pub mod reviewsession;
pub mod textinput;
pub mod uploader;
pub mod yesno;

pub trait Overlay: Komponent {
    fn is_done(&self) -> Signal<bool>;
}
