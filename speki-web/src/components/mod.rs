mod backside;
mod cardref;
mod dropdown;
mod frontside;
mod graph;

pub use backside::BackPut;
pub use cardref::CardRef;
use dioxus::prelude::Element;
pub use dropdown::DropDownMenu;
pub use frontside::{CardTy, FrontPut};
pub use graph::GraphRep;

pub trait Komponent {
    fn render(&self) -> Element;
}
