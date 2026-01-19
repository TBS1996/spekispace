use std::{
    collections::{BTreeSet, HashMap},
    hash::Hash,
    ops::Deref,
    sync::Arc,
};

use crate::LedgerItem;

/// Represents an item saved to disk, along with its reference cache.
/// Note: Ordering/hash/equality is based on the item's ID only.
#[derive(Clone, Debug)]
pub struct SavedItem<T: LedgerItem> {
    item: Arc<T>,
    #[allow(dead_code)]
    refs: Arc<HashMap<T::RefType, BTreeSet<T::Key>>>,
}

impl<T: LedgerItem> SavedItem<T>
where
    T: LedgerItem,
{
    pub fn new(item: T) -> Self {
        let mut refmap: HashMap<T::RefType, BTreeSet<T::Key>> = HashMap::new();
        for reff in item.ref_cache() {
            refmap.entry(reff.ty).or_default().insert(reff.to);
        }

        Self {
            item: Arc::new(item),
            refs: Arc::new(refmap),
        }
    }

    pub fn into_inner(self) -> T {
        Arc::unwrap_or_clone(self.item)
    }

    pub fn clone_inner(&self) -> T {
        (*self.item).clone()
    }

    pub fn item(&self) -> &Arc<T> {
        &self.item
    }
}

impl<T: LedgerItem> Hash for SavedItem<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.item.item_id().hash(state);
    }
}

impl<T: LedgerItem> PartialEq for SavedItem<T> {
    fn eq(&self, other: &Self) -> bool {
        self.item.item_id() == other.item.item_id()
    }
}

impl<T: LedgerItem> PartialOrd for SavedItem<T>
where
    T: LedgerItem,
{
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(Ord::cmp(&self.item.item_id(), &other.item.item_id()))
    }
}

impl<T: LedgerItem> Eq for SavedItem<T> where T: LedgerItem {}

impl<T: LedgerItem> Ord for SavedItem<T>
where
    T: LedgerItem,
{
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        Ord::cmp(&self.item.item_id(), &other.item.item_id())
    }
}

impl<T: LedgerItem> Deref for SavedItem<T>
where
    T: LedgerItem,
{
    type Target = Arc<T>;

    fn deref(&self) -> &Self::Target {
        &self.item
    }
}
