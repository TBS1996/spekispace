use crate::js;
use crate::load_cached_info;
use crate::load_user_info;
use crate::log_to_console;
use crate::provider::IndexBaseProvider;
use crate::ReviewState;
use crate::State;
use dioxus::prelude::*;
use speki_dto::{CardId, Recall, Review, SpekiProvider};
use std::time::Duration;
use uuid::Uuid;

mod add_card;
mod debug;
mod home;
mod review;
mod view;

pub const REMOTE: &'static str = "https://github.com/tbs1996/talecast.git";

pub use add_card::Add;
pub use debug::Debug;
pub use home::Home;
pub use review::Review;
pub use view::View;
