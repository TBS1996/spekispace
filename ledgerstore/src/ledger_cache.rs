

use crate::{Ledger, LedgerEvent, LedgerItem};

#[derive(Clone)]
pub struct LedgerCache<T: LedgerItem<E>, E: LedgerEvent> {
    ledger: Ledger<T, E>,
}
