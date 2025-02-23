use std::collections::{BTreeMap, BTreeSet};

use crate::card::CardId;


pub struct Cache {
    dependents: BTreeMap<CardId, BTreeSet<CardId>>,
}


