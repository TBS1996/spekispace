use crate::{ItemReference, LedgerItem};

pub trait WriteLedger {
    type Item: LedgerItem;

    fn remove(&self, key: <Self::Item as LedgerItem>::Key);
    fn save(&self, item: Self::Item);
    fn remove_property(
        &self,
        key: <Self::Item as LedgerItem>::Key,
        property: <Self::Item as LedgerItem>::PropertyType,
        value: String,
    );
    fn insert_property(
        &self,
        key: <Self::Item as LedgerItem>::Key,
        property: <Self::Item as LedgerItem>::PropertyType,
        value: String,
    );
    fn insert_reference(&self, reference: ItemReference<Self::Item>);
    fn remove_reference(&self, reference: ItemReference<Self::Item>);

    fn set_dependencies(&self, item: &Self::Item);
}
