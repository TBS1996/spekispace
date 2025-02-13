
use std::{collections::BTreeSet, time::Duration};

use serde::{Deserialize, Serialize};
use speki_dto::{Item, ModifiedSource};

use crate::card::CardId;

#[derive(Clone, Copy, PartialEq, PartialOrd, Debug, Serialize, Deserialize, Hash, Ord, Eq)]
pub struct Bigram([char;2]);

impl Bigram {
    pub fn new(a: char, b: char) -> Self {
        Self([a, b])
    }
}

impl ToString for Bigram{
    fn to_string(&self) -> String {
        serde_json::to_string(&self.0).unwrap()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Index {
    id: Bigram,
    pub deps: BTreeSet<CardId>,
    source: ModifiedSource,
    last_modified: Duration,
}

impl Index {
    pub fn new(id: Bigram, deps: BTreeSet<CardId>, current_time: Duration) -> Self {
        Self {
            id,
            deps,
            source: Default::default(),
            last_modified: current_time,
        }
    }
}


impl Item for Index {
    type PreviousVersion = Index;
    type Key = Bigram;

    fn deleted(&self) -> bool {
        false
    }

    fn set_delete(&mut self) {
        panic!("don't delete dependents cache!")
    }

    fn set_last_modified(&mut self, time: std::time::Duration) {
        self.last_modified = time;
    }

    fn last_modified(&self) -> std::time::Duration {
        self.last_modified
    }

    fn id(&self) -> Self::Key{
        self.id
    }

    fn identifier() -> &'static str {
        "dependents"
    }

    fn source(&self) -> ModifiedSource {
        self.source
    }

    fn set_source(&mut self, source: ModifiedSource) {
        self.source = source;
    }
}