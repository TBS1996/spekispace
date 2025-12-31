use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use crate::{
    blockchain::LedgerEvent, ledger_item::LedgerItem, read_ledger::ReadLedger, Ledger,
    PropertyCache,
};

/// Tracks changes to reference caches (dependencies/dependents) in a staging area
struct ReferenceCacheDelta<T: LedgerItem> {
    added_dependencies: HashMap<T::Key, HashMap<T::RefType, HashSet<T::Key>>>,
    removed_dependencies: HashMap<T::Key, HashMap<T::RefType, HashSet<T::Key>>>,
    added_dependents: HashMap<T::Key, HashMap<T::RefType, HashSet<T::Key>>>,
    removed_dependents: HashMap<T::Key, HashMap<T::RefType, HashSet<T::Key>>>,
}

impl<T: LedgerItem> ReferenceCacheDelta<T> {
    /// Get added references (dependencies or dependents) with type information - direct only
    fn get_added(
        &self,
        key: T::Key,
        ty: Option<&T::RefType>,
        reversed: bool,
    ) -> HashSet<(T::RefType, T::Key)> {
        let mut result = HashSet::new();
        let types_map = if reversed {
            self.added_dependents.get(&key)
        } else {
            self.added_dependencies.get(&key)
        };

        if let Some(types_map) = types_map {
            match ty {
                Some(filter_ty) => {
                    if let Some(refs) = types_map.get(filter_ty) {
                        for ref_key in refs {
                            result.insert((filter_ty.clone(), ref_key.clone()));
                        }
                    }
                }
                None => {
                    for (ref_ty, refs) in types_map {
                        for ref_key in refs {
                            result.insert((ref_ty.clone(), ref_key.clone()));
                        }
                    }
                }
            }
        }

        result
    }

    /// Get removed references (dependencies or dependents) with type information - direct only
    fn get_removed(
        &self,
        key: T::Key,
        ty: Option<&T::RefType>,
        reversed: bool,
    ) -> HashSet<(T::RefType, T::Key)> {
        let mut result = HashSet::new();
        let types_map = if reversed {
            self.removed_dependents.get(&key)
        } else {
            self.removed_dependencies.get(&key)
        };

        if let Some(types_map) = types_map {
            match ty {
                Some(filter_ty) => {
                    if let Some(refs) = types_map.get(filter_ty) {
                        for ref_key in refs {
                            result.insert((filter_ty.clone(), ref_key.clone()));
                        }
                    }
                }
                None => {
                    for (ref_ty, refs) in types_map {
                        for ref_key in refs {
                            result.insert((ref_ty.clone(), ref_key.clone()));
                        }
                    }
                }
            }
        }

        result
    }
}

pub struct StagingLedger<T: LedgerItem> {
    pub base: Ledger<T>,
    pub events: Vec<LedgerEvent<T>>,
    // How the staged events will modify items in base ledger.
    pub modified_items: HashMap<T::Key, Option<Arc<T>>>, // None means deleted
    added_properties: HashMap<PropertyCache<T>, HashSet<T::Key>>,
    removed_properties: HashMap<PropertyCache<T>, HashSet<T::Key>>,
    reference_deltas: ReferenceCacheDelta<T>,
}

impl<T: LedgerItem> StagingLedger<T> {
    fn collect_references_recursive(
        &self,
        key: T::Key,
        ty: Option<T::RefType>,
        out: &mut HashSet<T::Key>,
        reversed: bool,
    ) {
        // Get direct references (non-recursive) considering staged changes
        let direct_refs = self.get_reference_cache(key, ty.clone(), reversed, false);

        for ref_key in direct_refs {
            if out.insert(ref_key.clone()) {
                // Recursively collect from this reference
                self.collect_references_recursive(ref_key, ty.clone(), out, reversed);
            }
        }
    }

    fn collect_references_recursive_with_ty(
        &self,
        key: T::Key,
        ty: Option<T::RefType>,
        out: &mut HashSet<(T::RefType, T::Key)>,
        reversed: bool,
    ) {
        // Get direct references (non-recursive) considering staged changes
        let direct_refs = self.get_reference_cache_with_ty(key, ty.clone(), reversed, false);

        for (ref_ty, ref_key) in direct_refs {
            if out.insert((ref_ty.clone(), ref_key.clone())) {
                // Recursively collect from this reference
                self.collect_references_recursive_with_ty(ref_key, ty.clone(), out, reversed);
            }
        }
    }
}

impl<T: LedgerItem> ReadLedger for StagingLedger<T> {
    type Item = T;

    fn load(&self, key: <Self::Item as LedgerItem>::Key) -> Option<Self::Item> {
        if let Some(modified) = self.modified_items.get(&key) {
            match modified {
                Some(item) => Some((**item).clone()),
                None => None,
            }
        } else {
            self.base.load(key).map(|x| (*x).clone())
        }
    }

    fn load_ids(&self) -> HashSet<<Self::Item as LedgerItem>::Key> {
        let mut ids = self.base.load_ids();

        for (id, item) in self.modified_items.iter() {
            if item.is_some() {
                ids.insert(*id);
            } else {
                ids.remove(id);
            }
        }

        ids
    }

    fn get_property_cache(
        &self,
        cache: PropertyCache<Self::Item>,
    ) -> HashSet<<Self::Item as LedgerItem>::Key> {
        let mut out = self.base.get_prop_cache(cache.clone());

        if let Some(added) = self.added_properties.get(&cache) {
            out.extend(added.iter());
        }

        if let Some(removed) = self.removed_properties.get(&cache) {
            for key in removed.iter() {
                out.remove(key);
            }
        }

        out
    }

    fn get_reference_cache(
        &self,
        key: <Self::Item as LedgerItem>::Key,
        ty: Option<<Self::Item as LedgerItem>::RefType>,
        reversed: bool,
        recursive: bool,
    ) -> HashSet<<Self::Item as LedgerItem>::Key> {
        if recursive {
            let mut result = HashSet::new();
            self.collect_references_recursive(key, ty, &mut result, reversed);
            result
        } else {
            // Get direct references from base (non-recursive)
            let mut refs = self
                .base
                .get_reference_cache(key.clone(), ty.clone(), reversed, false);

            // Add staged additions (direct only)
            for (_ty, added_key) in
                self.reference_deltas
                    .get_added(key.clone(), ty.as_ref(), reversed)
            {
                refs.insert(added_key);
            }

            // Remove staged removals (direct only)
            for (_ty, removed_key) in self
                .reference_deltas
                .get_removed(key, ty.as_ref(), reversed)
            {
                refs.remove(&removed_key);
            }

            refs
        }
    }

    fn get_reference_cache_with_ty(
        &self,
        key: <Self::Item as LedgerItem>::Key,
        ty: Option<<Self::Item as LedgerItem>::RefType>,
        reversed: bool,
        recursive: bool,
    ) -> HashSet<(
        <Self::Item as LedgerItem>::RefType,
        <Self::Item as LedgerItem>::Key,
    )> {
        if recursive {
            let mut result = HashSet::new();
            self.collect_references_recursive_with_ty(key, ty, &mut result, reversed);
            result
        } else {
            // Get direct references from base (non-recursive)
            let mut refs =
                self.base
                    .get_reference_cache_with_ty(key.clone(), ty.clone(), reversed, false);

            // Add staged additions (direct only)
            for added in self
                .reference_deltas
                .get_added(key.clone(), ty.as_ref(), reversed)
            {
                refs.insert(added);
            }

            // Remove staged removals (direct only)
            for removed in self
                .reference_deltas
                .get_removed(key, ty.as_ref(), reversed)
            {
                refs.remove(&removed);
            }

            refs
        }
    }
}
