use std::{collections::BTreeSet, fmt::Display};

use ledgerstore::{LedgerEvent, LedgerItem};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumDiscriminants, EnumIter, EnumString};
use uuid::Uuid;

use crate::{card::CardId, collection::DynCard};

impl Display for Set {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.name)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Hash)]
pub enum SetAction {
    SetName(String),
    SetExpr(SetExpr),
    AddInput(Input),
}

pub type SetEvent = LedgerEvent<Set>;

impl LedgerItem for Set {
    type Error = ();
    type Key = SetId;

    type Modifier = SetAction;
    type RefType = String;

    type PropertyType = String;

    fn inner_run_event(mut self, event: SetAction) -> Result<Self, Self::Error> {
        match event {
            SetAction::SetName(name) => self.name = name,
            SetAction::SetExpr(expr) => self.expr = expr,
            SetAction::AddInput(input) => {
                let new_set = match self.expr {
                    SetExpr::Union(mut set) => {
                        set.insert(input);
                        SetExpr::Union(set)
                    }
                    SetExpr::Intersection(set) => {
                        let mut union: BTreeSet<Input> = Default::default();
                        union.insert(input);
                        union.insert(Input::Expr(Box::new(SetExpr::Intersection(set))));
                        SetExpr::Union(union)
                    }
                    SetExpr::Difference(diff1, diff2) => {
                        let mut union: BTreeSet<Input> = Default::default();
                        union.insert(input);
                        union.insert(Input::Expr(Box::new(SetExpr::Difference(diff1, diff2))));
                        SetExpr::Union(union)
                    }
                    SetExpr::Complement(cmp) => {
                        let mut union: BTreeSet<Input> = Default::default();
                        union.insert(input);
                        union.insert(Input::Expr(Box::new(SetExpr::Complement(cmp))));
                        SetExpr::Union(union)
                    }
                    SetExpr::All => SetExpr::All,
                };

                self.expr = new_set;
            }
        }

        Ok(self)
    }

    fn new_default(id: Self::Key) -> Self {
        Self {
            id,
            name: "...".to_string(),
            expr: SetExpr::Union(Default::default()),
        }
    }

    fn item_id(&self) -> Self::Key {
        self.id
    }
}

pub type SetId = Uuid;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Hash, Eq)]
pub struct Set {
    pub id: SetId,
    pub name: String,
    pub expr: SetExpr,
}

impl Set {
    /// Cards created from the CLI with --add flag will go into this set.
    pub const CLI_CARDS: Uuid = Uuid::from_u128(0xf5c1ef55_ebcd_40a4_9a12_f16e6d44b7a1);

    pub fn all_cards() -> Self {
        Self {
            id: Uuid::nil(),
            name: "all cards".to_string(),
            expr: SetExpr::All,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Hash, Ord, PartialOrd, Eq)]
pub enum Input {
    Card(CardId),
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
    All,
}

impl Default for SetExpr {
    fn default() -> Self {
        Self::Union(Default::default())
    }
}

impl SetExpr {
    pub fn universe() -> Self {
        Self::All
    }

    pub fn union_with(dyns: impl IntoIterator<Item = DynCard>) -> Self {
        let leafs: BTreeSet<Input> = dyns.into_iter().map(Input::Leaf).collect();
        Self::Union(leafs)
    }

    pub fn set_name(&self) -> &'static str {
        match self {
            SetExpr::Union(_) => "union",
            SetExpr::Intersection(_) => "intersection",
            SetExpr::Difference(_, _) => "difference",
            SetExpr::Complement(_) => "complement",
            SetExpr::All => "all",
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
            SetExpr::All => {}
        }
        out
    }
}
