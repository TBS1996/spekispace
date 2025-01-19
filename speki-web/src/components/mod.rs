pub mod backside;
mod cardref;
pub mod dropdown;
mod filtereditor;
mod frontside;
mod graph;

pub use backside::BackPut;
pub use cardref::CardRef;
use dioxus::prelude::Element;
pub use dropdown::DropDownMenu;
pub use filtereditor::*;
pub use frontside::{CardTy, FrontPut};
pub use graph::GraphRep;

pub trait Komponent {
    fn render(&self) -> Element;
}
