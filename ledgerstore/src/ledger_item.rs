use serde::{de::DeserializeOwned, Serialize};
use std::fmt::Display;
use std::str::FromStr;
use std::vec::Vec;
use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    hash::Hash,
};
use tracing::trace;

use crate::LedgerType;
use crate::OverrideLedger;
use crate::{CacheKey, EventError, ItemRefCache, PropertyCache};
use crate::{ItemReference, Ledger};

/// Represents how a ledger mutates or creates an item.
pub trait LedgerItem:
    Serialize + DeserializeOwned + Hash + Clone + Debug + Send + Sync + Eq + PartialEq + 'static
{
    type Key: Copy
        + Eq
        + Hash
        + ToString
        + Debug
        + Serialize
        + DeserializeOwned
        + FromStr
        + Send
        + Sync;
    type Error: Debug;

    /// The different ways an item can reference another item
    type RefType: AsRef<str>
        + Display
        + Clone
        + Hash
        + PartialEq
        + Eq
        + Send
        + Sync
        + Debug
        + FromStr;

    /// Cache regarding property of a card so you get like all the cards that have a certain value or whatever
    type PropertyType: AsRef<str> + Display + Clone + Hash + PartialEq + Eq + Send + Sync + Debug;

    /// The type that is responsible for mutating the item and thus create a new genreation
    type Modifier: Clone + Debug + Hash + Serialize + DeserializeOwned + Send + Sync;

    /// Modifies `Self`.
    fn inner_run_event(self, event: Self::Modifier) -> Result<Self, Self::Error>;

    fn verify(self, ledger: &Ledger<Self>) -> Result<Self, EventError<Self>> {
        let ledger = LedgerType::OverRide(OverrideLedger::new(ledger, self.clone()));

        if let Some(cycle) = self.find_cycle(&ledger) {
            return Err(EventError::Cycle(cycle));
        }

        if let Err(e) = self.validate(&ledger) {
            return Err(EventError::Invariant(e));
        }

        for dep in self.recursive_dependents(&ledger) {
            if let Err(e) = dep.validate(&ledger) {
                return Err(EventError::Invariant(e));
            }
        }

        Ok(self)
    }

    /// Modifies `Self` and checks for cycles and invariants.
    fn run_event(
        self,
        event: Self::Modifier,
        ledger: &Ledger<Self>,
        verify: bool,
    ) -> Result<Self, EventError<Self>> {
        let new = match self.inner_run_event(event) {
            Ok(item) => item,
            Err(e) => return Err(EventError::Invariant(e)),
        };

        if verify {
            new.verify(ledger)
        } else {
            Ok(new)
        }
    }

    fn new_default(id: Self::Key) -> Self;

    fn item_id(&self) -> Self::Key;

    fn find_cycle(&self, ledger: &LedgerType<Self>) -> Option<Vec<(Self::Key, Self::RefType)>> {
        fn dfs<T: LedgerItem>(
            current: T::Key,
            ledger: &LedgerType<T>,
            visiting: &mut HashSet<T::Key>,
            visited: &mut HashSet<T::Key>,
            parent: &mut HashMap<T::Key, (T::Key, T::RefType)>,
            selv: (T::Key, &T),
        ) -> Option<Vec<(T::Key, T::RefType)>> {
            if !visiting.insert(current.clone()) {
                // Cycle detected
                let mut path = Vec::new();
                let mut cur = current.clone();

                // Backtrack the path
                while let Some((p_key, p_ref)) = parent.get(&cur) {
                    path.push((cur.clone(), p_ref.clone()));
                    if *p_key == current {
                        break;
                    }
                    cur = p_key.clone();
                }

                path.reverse();
                return Some(path);
            }

            let dependencies = if selv.0 == current {
                selv.1.ref_cache()
            } else {
                ledger.load(current).unwrap().ref_cache()
            };

            for ItemReference {
                from: _,
                to: dep_key,
                ty: dep_type,
            } in dependencies
            {
                if visited.contains(&dep_key) {
                    continue;
                }
                parent.insert(dep_key.clone(), (current.clone(), dep_type.clone()));
                if let Some(cycle) = dfs(dep_key, ledger, visiting, visited, parent, selv) {
                    return Some(cycle);
                }
            }

            visiting.remove(&current);
            visited.insert(current);
            None
        }

        let mut visited = HashSet::new();
        let mut visiting = HashSet::new();
        let mut parent = HashMap::new();

        dfs(
            self.item_id(),
            ledger,
            &mut visiting,
            &mut visited,
            &mut parent,
            (self.item_id(), self),
        )
    }

    /// Assertions that should hold true. Like invariants with other cards that it references.
    /// called by run_event, if it returns error after an event is run, the event is not applied.
    fn validate(&self, ledger: &LedgerType<Self>) -> Result<(), Self::Error> {
        let _ = ledger;
        Ok(())
    }

    /// List of references to other items, along with the name of the type of reference.
    ///
    /// Used to create a index, like if item A references item B, we cache that item B is referenced by item A,
    /// so that we don't need to search through all the items to find out or store it double in the item itself.
    fn ref_cache(&self) -> HashSet<ItemReference<Self>> {
        Default::default()
    }

    fn dependencies(&self) -> HashSet<Self::Key> {
        self.ref_cache()
            .into_iter()
            .map(|itemref| itemref.to)
            .collect()
    }

    fn recursive_dependent_ids(&self, ledger: &LedgerType<Self>) -> HashSet<Self::Key>
    where
        Self: Sized,
    {
        let mut out: HashSet<Self::Key> = HashSet::new();
        let mut visited: HashSet<Self::Key> = HashSet::new();

        fn visit<T: LedgerItem>(
            key: T::Key,
            ledger: &LedgerType<T>,
            out: &mut HashSet<T::Key>,
            visited: &mut HashSet<T::Key>,
        ) where
            T: Sized,
        {
            if !visited.insert(key) {
                return;
            }

            out.insert(key);

            for dep_key in ledger.dependents(key) {
                visit(dep_key, ledger, out, visited);
            }
        }

        for dep_key in self.dependents(&ledger) {
            visit(dep_key, &ledger, &mut out, &mut visited);
        }

        out
    }

    fn recursive_dependents(&self, ledger: &LedgerType<Self>) -> HashSet<Self>
    where
        Self: Sized,
    {
        let mut out: HashSet<Self> = HashSet::new();
        let mut visited: HashSet<Self::Key> = HashSet::new();

        fn visit<T: LedgerItem>(
            key: T::Key,
            ledger: &LedgerType<T>,
            out: &mut HashSet<T>,
            visited: &mut HashSet<T::Key>,
        ) where
            T: Sized,
        {
            if !visited.insert(key) {
                return;
            }
            let item = ledger.load(key).unwrap();

            out.insert(item.clone());

            for dep_key in item.dependents(ledger) {
                visit(dep_key, ledger, out, visited);
            }
        }

        for dep_key in self.dependents(&ledger) {
            visit(dep_key, &ledger, &mut out, &mut visited);
        }

        out
    }

    fn dependents(&self, ledger: &LedgerType<Self>) -> HashSet<Self::Key> {
        match ledger {
            LedgerType::OverRide(ledger) => ledger.dependents(self.item_id()),
            LedgerType::Normal(ledger) => ledger.all_dependents(self.item_id()),
        }
    }

    /// List of defined properties that this item has.
    ///
    /// The property keys are predefined, hence theyre static str
    /// the String is the Value which could be anything.
    /// For example ("suspended", true).
    fn properties_cache(&self, ledger: &Ledger<Self>) -> HashSet<PropertyCache<Self>>
    where
        Self: LedgerItem,
    {
        let _ = ledger;
        Default::default()
    }

    fn listed_cache(&self, ledger: &Ledger<Self>) -> HashMap<CacheKey<Self>, HashSet<Self::Key>> {
        let mut out: HashMap<CacheKey<Self>, HashSet<Self::Key>> = HashMap::default();

        for (key, id) in self.caches(ledger) {
            out.entry(key).or_default().insert(id);
        }

        out
    }

    fn caches(&self, ledger: &Ledger<Self>) -> HashSet<(CacheKey<Self>, Self::Key)>
    where
        Self: LedgerItem,
    {
        trace!("fetching caches for item: {:?}", self.item_id());

        let mut out: HashSet<(CacheKey<Self>, Self::Key)> = Default::default();
        let id = self.item_id();

        for property_cache in self.properties_cache(ledger) {
            out.insert((CacheKey::Left(property_cache), id.clone()));
        }

        for ItemReference { from, to, ty } in self.ref_cache() {
            out.insert((
                CacheKey::Right(ItemRefCache {
                    reftype: ty.clone(),
                    id: to,
                }),
                from,
            ));
        }

        out
    }
}
