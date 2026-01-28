use indexmap::IndexSet;
use std::path::PathBuf;

use crate::read_ledger::{FsReadLedger, ReadLedger};
use crate::{LedgerItem, PropertyCache};

#[derive(Clone)]
pub struct Local<T: LedgerItem> {
    pub inner: FsReadLedger<T>,
}

impl<T: LedgerItem> Local<T> {
    pub fn item_path(&self, key: T::Key) -> PathBuf {
        self.inner.item_path(key)
    }

    pub fn item_path_create(&self, key: T::Key) -> PathBuf {
        self.inner.item_path_create(key)
    }
}

impl<T: LedgerItem> ReadLedger for Local<T> {
    type Item = T;

    fn load(&self, key: <Self::Item as LedgerItem>::Key) -> Option<Self::Item> {
        self.inner.load(key)
    }

    fn load_ids(&self) -> IndexSet<<Self::Item as LedgerItem>::Key> {
        self.inner.load_ids()
    }

    fn get_property_cache(
        &self,
        cache: PropertyCache<Self::Item>,
    ) -> IndexSet<<Self::Item as LedgerItem>::Key> {
        self.inner.get_property_cache(cache)
    }

    fn has_property(
        &self,
        key: <Self::Item as LedgerItem>::Key,
        property: PropertyCache<Self::Item>,
    ) -> bool {
        self.inner.has_property(key, property)
    }

    fn get_reference_cache(
        &self,
        key: <Self::Item as LedgerItem>::Key,
        ty: Option<<Self::Item as LedgerItem>::RefType>,
        reversed: bool,
        recursive: bool,
    ) -> IndexSet<<Self::Item as LedgerItem>::Key> {
        self.inner.get_reference_cache(key, ty, reversed, recursive)
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
        self.inner
            .get_reference_cache_with_ty(key, ty, reversed, recursive)
    }
}
