use indexmap::IndexSet;
use serde::{de::DeserializeOwned, Serialize};
use std::fmt::Display;
use std::str::FromStr;
use std::sync::Arc;
use std::vec::Vec;
use std::{collections::HashMap, fmt::Debug, hash::Hash};
use tracing::trace;

use crate::ItemReference;
use crate::ReadLedger;
use crate::{CacheKey, EventError, ItemRefCache, PropertyCache};

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
        + Sync
        + Ord;
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
        + FromStr
        + Serialize
        + DeserializeOwned;

    /// Cache regarding property of a card so you get like all the cards that have a certain value or whatever
    type PropertyType: AsRef<str>
        + Display
        + Clone
        + Hash
        + PartialEq
        + Eq
        + Send
        + Sync
        + Debug
        + Serialize
        + DeserializeOwned;

    /// The type that is responsible for mutating the item and thus create a new genreation
    type Modifier: Clone + Debug + Hash + Serialize + DeserializeOwned + Send + Sync;

    /// Modifies `Self`.
    fn inner_run_action(self, event: Self::Modifier) -> Result<Self, Self::Error>;

    fn verify_with_deps(
        self,
        ledger: &impl ReadLedger<Item = Self>,
    ) -> Result<(Self, Vec<Self::Key>), EventError<Self>> {
        if let Some(cycle) = self.find_cycle(ledger) {
            return Err(EventError::Cycle(cycle));
        }

        if let Err(e) = self.validate(ledger) {
            return Err(EventError::Invariant(e));
        }

        let mut deps: Vec<Self::Key> = vec![];
        for dep in self.recursive_dependents(ledger) {
            deps.push(dep.item_id());
            if let Err(e) = dep.validate(ledger) {
                return Err(EventError::Invariant(e));
            }
        }

        Ok((self, deps))
    }

    fn verify(self, ledger: &impl ReadLedger<Item = Self>) -> Result<Self, EventError<Self>> {
        self.verify_with_deps(ledger).map(|(selv, _)| selv)
    }

    /// Modifies `Self` and checks for cycles and invariants.
    fn run_event(
        self,
        event: Self::Modifier,
        ledger: &impl ReadLedger<Item = Self>,
        verify: bool,
    ) -> Result<Self, EventError<Self>> {
        let new = match self.inner_run_action(event) {
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

    fn find_cycle(
        &self,
        ledger: &impl ReadLedger<Item = Self>,
    ) -> Option<Vec<(Self::Key, Self::RefType)>> {
        fn dfs<T: LedgerItem>(
            current: T::Key,
            ledger: &impl ReadLedger<Item = T>,
            visiting: &mut IndexSet<T::Key>,
            visited: &mut IndexSet<T::Key>,
            parent: &mut HashMap<T::Key, (T::Key, T::RefType)>,
            selv: (T::Key, &T),
        ) -> Option<Vec<(T::Key, T::RefType)>> {
            if !visiting.insert(current.clone()) {
                // Cycle detected
                // Check if this is a self-loop (node depends on itself)
                if let Some((p_key, _)) = parent.get(&current) {
                    if *p_key == current {
                        // Self-loop detected - this is allowed
                        return None;
                    }
                }

                // Build the cycle path by backtracking through parent pointers
                let mut path = Vec::new();
                let cycle_start = current.clone();

                // Backtrack to build the full cycle
                let mut cur = current.clone();
                loop {
                    if let Some((p_key, p_ref)) = parent.get(&cur) {
                        path.push((cur.clone(), p_ref.clone()));
                        if p_key == &cycle_start {
                            // We've completed the cycle back to the start
                            break;
                        }
                        cur = p_key.clone();
                    } else {
                        // Shouldn't happen in a proper cycle
                        break;
                    }
                }

                path.reverse();
                return Some(path);
            }

            let dependencies = if selv.0 == current {
                selv.1.ref_cache()
            } else {
                match ledger.load(current) {
                    Some(item) => item.ref_cache(),
                    None => {
                        dbg!(current, visited, visiting, parent);
                        panic!()
                    }
                }
            };

            for ItemReference {
                from: _,
                to: dep_key,
                ty: dep_type,
            } in dependencies
            {
                // Skip self-loops - items are allowed to depend on themselves
                if dep_key == current {
                    continue;
                }

                if visited.contains(&dep_key) {
                    continue;
                }
                parent.insert(dep_key.clone(), (current.clone(), dep_type.clone()));
                if let Some(cycle) = dfs(dep_key, ledger, visiting, visited, parent, selv) {
                    return Some(cycle);
                }
            }

            visiting.shift_remove(&current);
            visited.insert(current);
            None
        }

        let mut visited = IndexSet::new();
        let mut visiting = IndexSet::new();
        let mut parent = HashMap::new();

        let cycle = dfs(
            self.item_id(),
            ledger,
            &mut visiting,
            &mut visited,
            &mut parent,
            (self.item_id(), self),
        );

        cycle
    }

    /// Assertions that should hold true. Like invariants with other cards that it references.
    /// called by run_event, if it returns error after an event is run, the event is not applied.
    fn validate(&self, ledger: &impl ReadLedger<Item = Self>) -> Result<(), Self::Error> {
        let _ = ledger;
        Ok(())
    }

    /// List of references to other items, along with the name of the type of reference.
    ///
    /// Used to create a index, like if item A references item B, we cache that item B is referenced by item A,
    /// so that we don't need to search through all the items to find out or store it double in the item itself.
    fn ref_cache(&self) -> IndexSet<ItemReference<Self>> {
        Default::default()
    }

    fn dependencies(&self) -> IndexSet<Self::Key> {
        self.ref_cache()
            .into_iter()
            .map(|itemref| itemref.to)
            .collect()
    }

    fn recursive_dependents(&self, ledger: &impl ReadLedger<Item = Self>) -> IndexSet<Arc<Self>>
    where
        Self: Sized,
    {
        let mut out: IndexSet<Arc<Self>> = IndexSet::new();
        let mut visited: IndexSet<Self::Key> = IndexSet::new();

        fn visit<T: LedgerItem>(
            key: T::Key,
            ledger: &impl ReadLedger<Item = T>,
            out: &mut IndexSet<Arc<T>>,
            visited: &mut IndexSet<T::Key>,
        ) where
            T: Sized,
        {
            if !visited.insert(key) {
                return;
            }
            let item = Arc::new(ledger.load(key).unwrap());

            out.insert(item.clone());

            // Get dependents from the reference cache
            let dep_keys = ledger.get_reference_cache(key, None, true, false);
            for dep_key in dep_keys {
                visit(dep_key, ledger, out, visited);
            }
        }

        // Get direct dependents of self
        let dep_keys = ledger.get_reference_cache(self.item_id(), None, true, false);
        for dep_key in dep_keys {
            visit(dep_key, ledger, &mut out, &mut visited);
        }

        out
    }

    /// List of defined properties that this item has.
    ///
    /// The property keys are predefined, hence theyre static str
    /// the String is the Value which could be anything.
    /// For example ("suspended", true).
    fn properties_cache(
        &self,
        ledger: &impl ReadLedger<Item = Self>,
    ) -> IndexSet<PropertyCache<Self>>
    where
        Self: LedgerItem,
    {
        let _ = ledger;
        Default::default()
    }

    fn listed_cache(
        &self,
        ledger: &impl ReadLedger<Item = Self>,
    ) -> HashMap<CacheKey<Self>, IndexSet<Self::Key>> {
        let mut out: HashMap<CacheKey<Self>, IndexSet<Self::Key>> = HashMap::default();

        for (key, id) in self.caches(ledger) {
            out.entry(key).or_default().insert(id);
        }

        out
    }

    fn caches(&self, ledger: &impl ReadLedger<Item = Self>) -> IndexSet<(CacheKey<Self>, Self::Key)>
    where
        Self: LedgerItem,
    {
        trace!("fetching caches for item: {:?}", self.item_id());

        let mut out: IndexSet<(CacheKey<Self>, Self::Key)> = Default::default();
        let id = self.item_id();

        for property_cache in self.properties_cache(ledger) {
            out.insert((CacheKey::Left(property_cache), id.clone()));
        }

        for ItemReference { from, to, ty } in self.ref_cache() {
            out.insert((
                CacheKey::Right(ItemRefCache {
                    reftype: ty.clone(),
                    referent: to,
                }),
                from,
            ));
        }

        out
    }
}
