use indexmap::IndexSet;
use std::{collections::HashMap, sync::Arc};

use crate::{
    blockchain::ItemAction, ledger_item::LedgerItem, read_ledger::ReadLedger, ActionEvalResult,
    CardChange, EventError, ItemRefCache, ItemReference, Ledger, LedgerAction, PropertyCache,
    SavedItem, WriteLedger,
};

/// Tracks changes to reference caches (dependencies/dependents) in a staging area
struct ReferenceCacheDelta<T: LedgerItem> {
    added_dependencies: HashMap<T::Key, HashMap<T::RefType, IndexSet<T::Key>>>,
    removed_dependencies: HashMap<T::Key, HashMap<T::RefType, IndexSet<T::Key>>>,
    added_dependents: HashMap<T::Key, HashMap<T::RefType, IndexSet<T::Key>>>,
    removed_dependents: HashMap<T::Key, HashMap<T::RefType, IndexSet<T::Key>>>,
}

impl<T: LedgerItem> ReferenceCacheDelta<T> {
    fn add(
        &mut self,
        ItemRefCache {
            referent: to,
            reftype: ty,
        }: ItemRefCache<T>,
        from: T::Key,
    ) {
        self.added_dependencies
            .entry(from)
            .or_default()
            .entry(ty.clone())
            .or_default()
            .insert(to);
        self.added_dependents
            .entry(to)
            .or_default()
            .entry(ty.clone())
            .or_default()
            .insert(from);

        self.removed_dependencies
            .entry(from)
            .or_default()
            .entry(ty.clone())
            .or_default()
            .shift_remove(&to);

        self.removed_dependents
            .entry(to)
            .or_default()
            .entry(ty)
            .or_default()
            .shift_remove(&from);
    }

    fn remove(
        &mut self,
        ItemRefCache {
            referent: to,
            reftype: ty,
        }: ItemRefCache<T>,
        from: T::Key,
    ) {
        self.removed_dependencies
            .entry(from)
            .or_default()
            .entry(ty.clone())
            .or_default()
            .insert(to);
        self.removed_dependents
            .entry(to)
            .or_default()
            .entry(ty.clone())
            .or_default()
            .insert(from);

        self.added_dependencies
            .entry(from)
            .or_default()
            .entry(ty.clone())
            .or_default()
            .shift_remove(&to);

        self.added_dependents
            .entry(to)
            .or_default()
            .entry(ty)
            .or_default()
            .shift_remove(&from);
    }

    /// Get added references (dependencies or dependents) with type information - direct only
    fn get_added(
        &self,
        key: T::Key,
        ty: Option<&T::RefType>,
        reversed: bool,
    ) -> IndexSet<(T::RefType, T::Key)> {
        let mut result = IndexSet::new();
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
    ) -> IndexSet<(T::RefType, T::Key)> {
        let mut result = IndexSet::new();
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

/// A ledger with a staging layer for uncommitted changes.
///
/// Tracks modifications to items, properties, and references without
/// persisting them to the base ledger until explicitly committed.
pub struct StagingLedger<T: LedgerItem> {
    pub base: Ledger<T>,
    events: Vec<(ItemAction<T>, CardChange<T>)>,
    // How the staged events will modify items in base ledger.
    pub modified_items: HashMap<T::Key, Option<Arc<T>>>, // None means deleted
    added_properties: HashMap<PropertyCache<T>, IndexSet<T::Key>>,
    removed_properties: HashMap<PropertyCache<T>, IndexSet<T::Key>>,
    reference_deltas: ReferenceCacheDelta<T>,
}

impl<T: LedgerItem> StagingLedger<T> {
    pub fn new(base: Ledger<T>) -> Self {
        Self {
            base,
            events: Vec::new(),
            modified_items: HashMap::new(),
            added_properties: HashMap::new(),
            removed_properties: HashMap::new(),
            reference_deltas: ReferenceCacheDelta {
                added_dependencies: HashMap::new(),
                removed_dependencies: HashMap::new(),
                added_dependents: HashMap::new(),
                removed_dependents: HashMap::new(),
            },
        }
    }

    pub fn push_event(&mut self, event: ItemAction<T>) -> Result<(), EventError<T>> {
        let res = self.evaluate_action(event.clone(), true, true)?;

        if res.is_no_op {
            // Sanity check: if it's a no-op, caches shouldn't have changed
            if !res.added_caches.is_empty() || !res.removed_caches.is_empty() {
                panic!(
                    "Inconsistent state: operation marked as no-op but caches changed. \
                     Added: {:?}, Removed: {:?}",
                    res.added_caches, res.removed_caches
                );
            }

            return Ok(());
        }

        self.events.push((event, res.item.clone()));
        self.update_layer(res);

        Ok(())
    }

    pub fn commit_events(self) -> Result<Vec<CardChange<T>>, EventError<T>> {
        let Self {
            modified_items: items,
            added_properties,
            removed_properties,
            base,
            events,
            reference_deltas,
        } = self;

        for (key, item) in items {
            match item {
                Some(item) => {
                    let item = Arc::unwrap_or_clone(item);
                    base.save(item.clone());
                    base.cache
                        .write()
                        .unwrap()
                        .insert(key, SavedItem::new(item));
                }
                None => {
                    base.cache.write().unwrap().remove(&key);
                    base.remove(key);
                }
            };
        }

        for (PropertyCache { property, value }, keys) in added_properties {
            for key in keys {
                base.insert_property(key, property.clone(), value.clone());
            }
        }

        for (PropertyCache { property, value }, keys) in removed_properties {
            for key in keys {
                base.remove_property(key, property.clone(), value.clone());
            }
        }

        for (from, val) in reference_deltas.added_dependencies {
            for (ref_type, referents) in val {
                for referent in referents {
                    let itemref: ItemReference<T> = ItemReference {
                        from,
                        to: referent,
                        ty: ref_type.clone(),
                    };

                    base.insert_reference(itemref);
                }
            }
        }

        for (from, val) in reference_deltas.removed_dependencies {
            for (ref_type, referents) in val {
                for referent in referents {
                    let itemref: ItemReference<T> = ItemReference {
                        from,
                        to: referent,
                        ty: ref_type.clone(),
                    };

                    base.remove_reference(itemref);
                }
            }
        }

        let (events, changes) = events.into_iter().unzip();

        base.save_events(events);

        Ok(changes)
    }

    fn update_layer(&mut self, res: ActionEvalResult<T>) {
        let ActionEvalResult {
            item,
            added_caches,
            removed_caches,
            is_no_op: _,
        } = res;

        match item {
            CardChange::Created(item) | CardChange::Modified(item) => {
                self.modified_items
                    .insert(item.item_id(), Some(item.clone()));
            }
            CardChange::Deleted(id) => {
                self.modified_items.insert(id, None);
            }
            CardChange::Unchanged(_) => return,
        };

        for (cache, key) in added_caches {
            match cache {
                either::Either::Left(prop) => {
                    self.added_properties
                        .entry(prop.clone())
                        .or_default()
                        .insert(key);
                    self.removed_properties
                        .entry(prop)
                        .or_default()
                        .shift_remove(&key);
                }
                either::Either::Right(reff) => {
                    self.reference_deltas.add(reff, key);
                }
            }
        }

        for (cache, key) in removed_caches {
            match cache {
                either::Either::Left(prop) => {
                    self.removed_properties
                        .entry(prop.clone())
                        .or_default()
                        .insert(key);
                    self.added_properties
                        .entry(prop)
                        .or_default()
                        .shift_remove(&key);
                }
                either::Either::Right(reff) => {
                    self.reference_deltas.remove(reff, key);
                }
            }
        }
    }

    /// See how a [`LedgerAction`] would change the state, without actually saving the result.
    fn evaluate_action(
        &self,
        ItemAction { id: key, action }: ItemAction<T>,
        verify: bool, // if true, check if action will uphold invariants
        cache: bool,  // if true, return cache modification results.
    ) -> Result<ActionEvalResult<T>, EventError<T>> {
        if self.base.is_remote(key) && verify {
            return Err(EventError::Remote);
        }

        let (old_caches, new_caches, item, is_no_op) = match action.clone() {
            LedgerAction::Modify(action) => {
                let (old_caches, old_item) = match self.load(key) {
                    Some(item) => {
                        let item = Arc::new(item);
                        let caches = if cache {
                            item.caches(self)
                        } else {
                            Default::default()
                        };
                        (caches, item)
                    }
                    None => (Default::default(), Arc::new(T::new_default(key))),
                };
                let old_cloned = old_item.clone();
                let modified_item =
                    Arc::new(Arc::unwrap_or_clone(old_item).run_event(action, self, verify)?);

                let no_op = old_cloned == modified_item;

                let item = if no_op {
                    CardChange::Unchanged(modified_item.item_id())
                } else {
                    CardChange::Modified(modified_item.clone())
                };

                let new_caches = modified_item.caches(self);
                (old_caches, new_caches, item, no_op)
            }
            LedgerAction::Create(mut item) => {
                if verify {
                    item = item.verify(self)?;
                }
                let caches = if cache {
                    item.caches(self)
                } else {
                    Default::default()
                };

                let item = Arc::new(item);

                (
                    IndexSet::default(),
                    caches,
                    CardChange::Created(item),
                    false,
                )
            }
            LedgerAction::Delete => {
                let old_item = self.load(key).unwrap();
                let old_caches = old_item.caches(self);
                (
                    old_caches,
                    Default::default(),
                    CardChange::Deleted(key),
                    false,
                )
            }
        };

        let added_caches = &new_caches - &old_caches;
        let removed_caches = &old_caches - &new_caches;

        Ok(ActionEvalResult {
            item,
            added_caches,
            removed_caches,
            is_no_op,
        })
    }

    fn collect_references_recursive(
        &self,
        key: T::Key,
        ty: Option<T::RefType>,
        out: &mut IndexSet<T::Key>,
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
        out: &mut IndexSet<(T::RefType, T::Key)>,
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

    fn load_ids(&self) -> IndexSet<<Self::Item as LedgerItem>::Key> {
        let mut ids = self.base.load_ids();

        for (id, item) in self.modified_items.iter() {
            if item.is_some() {
                ids.insert(*id);
            } else {
                ids.shift_remove(id);
            }
        }

        ids
    }

    fn get_property_cache(
        &self,
        cache: PropertyCache<Self::Item>,
    ) -> IndexSet<<Self::Item as LedgerItem>::Key> {
        let mut out = self.base.get_prop_cache(cache.clone());

        if let Some(added) = self.added_properties.get(&cache) {
            out.extend(added.iter());
        }

        if let Some(removed) = self.removed_properties.get(&cache) {
            for key in removed.iter() {
                out.shift_remove(key);
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
    ) -> IndexSet<<Self::Item as LedgerItem>::Key> {
        if recursive {
            let mut result = IndexSet::new();
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
                refs.shift_remove(&removed_key);
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
    ) -> IndexSet<(
        <Self::Item as LedgerItem>::RefType,
        <Self::Item as LedgerItem>::Key,
    )> {
        if recursive {
            let mut result = IndexSet::new();
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
                refs.shift_remove(&removed);
            }

            refs
        }
    }
}
