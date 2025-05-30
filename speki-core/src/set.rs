use std::{collections::BTreeSet, fmt::Display};

use ledgerstore::{LedgerEvent, LedgerItem};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumDiscriminants, EnumIter, EnumString};
use uuid::Uuid;

use crate::{
    card_provider::CardProvider,
    collection::{DynCard, MaybeCard},
};

impl Display for Set {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.name)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Hash)]
pub enum SetAction {
    SetName(String),
    SetExpr(SetExpr),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Hash)]
pub struct SetEvent {
    id: SetId,
    action: SetAction,
}

impl SetEvent {
    pub fn new(id: SetId, action: SetAction) -> Self {
        Self { id, action }
    }
}

impl LedgerEvent for SetEvent {
    type Key = SetId;

    fn id(&self) -> Self::Key {
        self.id
    }
}

impl LedgerItem<SetEvent> for Set {
    type Error = ();

    type RefType = String;

    type PropertyType = String;

    fn run_event(mut self, event: SetEvent) -> Result<Self, Self::Error> {
        match event.action {
            SetAction::SetName(name) => self.name = name,
            SetAction::SetExpr(expr) => self.expr = expr,
        }

        Ok(self)
    }

    fn new_default(id: <SetEvent as ledgerstore::LedgerEvent>::Key) -> Self {
        Self {
            id,
            name: "...".to_string(),
            expr: SetExpr::Union(Default::default()),
        }
    }

    fn item_id(&self) -> <SetEvent as ledgerstore::LedgerEvent>::Key {
        self.id
    }
}

pub type SetId = Uuid;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Hash)]
pub struct Set {
    pub id: SetId,
    pub name: String,
    pub expr: SetExpr,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Hash, Ord, PartialOrd, Eq)]
pub enum Input {
    Leaf(DynCard),
    Reference(SetId),
    Expr(Box<SetExpr>),
}

#[derive(
    Serialize, Deserialize, Debug, Clone, PartialEq, Hash, Eq, PartialOrd, Ord, EnumDiscriminants,
)]
#[strum_discriminants(derive(
    EnumIter,
    EnumString,
    Display,
    Serialize,
    Deserialize,
    PartialOrd,
    Ord
))]
pub enum SetExpr {
    Union(BTreeSet<Input>),
    Intersection(BTreeSet<Input>),
    Difference(Input, Input),
    Complement(Input),
}

impl Default for SetExpr {
    fn default() -> Self {
        Self::Union(Default::default())
    }
}

impl Input {
    pub fn eval(&self, provider: &CardProvider) -> BTreeSet<MaybeCard> {
        match self {
            Input::Leaf(dc) => dc.evaluate(provider.clone()).into_iter().collect(),
            Input::Reference(id) => provider
                .providers
                .sets
                .load(*id)
                .unwrap()
                .expr
                .eval(provider),
            Input::Expr(expr) => expr.eval(provider),
        }
    }
}

impl SetExpr {
    pub fn set_name(&self) -> &'static str {
        match self {
            SetExpr::Union(_) => "union",
            SetExpr::Intersection(_) => "intersection",
            SetExpr::Difference(_, _) => "difference",
            SetExpr::Complement(_) => "complement",
        }
    }

    pub fn inputs(&self) -> Vec<&Input> {
        let mut out = vec![];
        match self {
            SetExpr::Union(set) => out.extend(set),
            SetExpr::Intersection(set) => out.extend(set),
            SetExpr::Difference(input, input1) => {
                out.push(input);
                out.push(input1);
            }
            SetExpr::Complement(input) => out.push(input),
        }
        out
    }

    pub fn eval(&self, provider: &CardProvider) -> BTreeSet<MaybeCard> {
        match self {
            SetExpr::Union(hash_set) => {
                let mut out: BTreeSet<MaybeCard> = Default::default();
                for input in hash_set {
                    out.extend(input.eval(provider));
                }
                out
            }
            SetExpr::Intersection(hash_set) => {
                let mut iter = hash_set.into_iter();

                let Some(first) = iter.next() else {
                    return Default::default();
                };

                let mut set = first.eval(provider);

                for input in iter {
                    set = set.intersection(&input.eval(provider)).cloned().collect();
                }

                set
            }
            SetExpr::Difference(input1, input2) => {
                let set1 = input1.eval(provider);
                let set2 = input2.eval(provider);
                set1.difference(&set2).cloned().collect()
            }
            SetExpr::Complement(input) => provider
                .providers
                .cards
                .load_ids()
                .into_iter()
                .map(|id| MaybeCard::Id(id.parse().unwrap()))
                .collect::<BTreeSet<MaybeCard>>()
                .difference(&input.eval(provider))
                .cloned()
                .collect(),
        }
    }
}
