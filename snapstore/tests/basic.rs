/*
use std::{
    fs,
    path::{Path, PathBuf},
};

use ledgerstore::{Ledger, LedgerEvent, LedgerItem};
use std::ops::Deref;
use tracing::{Level, info};
use uuid::Uuid;

struct TestLedger<T: LedgerItem<E>, E: LedgerEvent> {
    ledger: Ledger<T, E>,
    root: PathBuf,
}

impl<T: LedgerItem<E>, E: LedgerEvent> TestLedger<T, E> {
    fn new() -> Self {
        let root =
            PathBuf::from("/home/tor/ledgertest").join(Uuid::new_v4().as_simple().to_string());
        Self {
            ledger: Ledger::new(root.as_path()),
            root,
        }
    }
}

impl<T: LedgerItem<E>, E: LedgerEvent> Deref for TestLedger<T, E> {
    type Target = Ledger<T, E>;

    fn deref(&self) -> &Self::Target {
        &self.ledger
    }
}

impl<T: LedgerItem<E>, E: LedgerEvent> Drop for TestLedger<T, E> {
    fn drop(&mut self) {
        //fs::remove_dir_all(&self.root).unwrap()
    }
}

fn new_card_with_id(front: &str, back: &str, id: CardId) -> CardEvent {
    let ty = CardType::Normal {
        front: front.to_string(),
        back: back.to_string().into(),
    };
    CardEvent::new(id, CardAction::UpsertCard(ty))
}

fn new_card(front: &str, back: &str) -> CardEvent {
    new_card_with_id(front, back, CardId::new_v4())
}

fn new_card_ledger() -> TestLedger<RawCard, CardEvent> {
    TestLedger::new()
}

/// hmm
/// so i guess the api should be, you just insert a ledger, it doesnt do anything until you try to get an item
/// itll then see theres unapplied entries, itll apply them and then use it
#[tokio::test]
async fn test_insert_retrieve_succ() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(Level::TRACE)
        .try_init()
        .unwrap();
    let ledger = new_card_ledger();
    std::thread::sleep(std::time::Duration::from_millis(200));
    let new_card = new_card_with_id(
        "foo",
        "bar",
        "de626024-07aa-42fb-8013-c198dba2c0a7".parse().unwrap(),
    );
    let id = new_card.id;
    ledger.insert_ledger(new_card).await;

    info!("fom test loading ledger");
    std::thread::sleep(std::time::Duration::from_millis(200));
    let res = ledger.load(&id.to_string()).await.unwrap();
    info!("finished: {res:?}");
}

/// hmm
/// so i guess the api should be, you just insert a ledger, it doesnt do anything until you try to get an item
/// itll then see theres unapplied entries, itll apply them and then use it
///
/// TEST FAIL CAUSE BIGRAM KEY COLLISION
#[tokio::test]
async fn test_insert_retrieve_fail() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(Level::TRACE)
        .try_init()
        .unwrap();
    let ledger = new_card_ledger();
    std::thread::sleep(std::time::Duration::from_millis(200));
    let new_card = new_card_with_id(
        "foo",
        "bar",
        "1843da5f-f44a-421d-bb81-fbf465173076".parse().unwrap(),
    );
    let id = new_card.id;
    ledger.insert_ledger(new_card).await;

    info!("fom test loading ledger");
    std::thread::sleep(std::time::Duration::from_millis(200));
    let res = ledger.load(&id.to_string()).await.unwrap();
    info!("finished: {res:?}");
}

#[tokio::test]
async fn test_ref_cache() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(Level::TRACE)
        .try_init()
        .ok();
    let ledger = new_card_ledger();
    let card_a = new_card("a", "bar");
    let a_id = card_a.id;
    ledger.insert_ledger(card_a).await;

    let card_b = new_card("b", "foo");
    let b_id = card_b.id;
    ledger.insert_ledger(card_b).await;

    let refaction = CardEvent::new(a_id, CardAction::AddDependency(b_id));
    ledger.insert_ledger(refaction).await;

    let card_a = ledger.load(&a_id.to_string()).await.unwrap();

    assert!(card_a.dependencies.contains(&b_id));
}
*/
