//! Tests for the Phase 1 journal store.

use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;
use vyapaar_core::{Currency, Money};

mod common;
use common::{now};
use vyapaar_ledger_performance::model::{
    AccountProvenance, JournalAccountId, PnlConvention, TradeAnnotation,
};
use vyapaar_ledger_performance::store::{JournalStore, StoreError};

fn money(s: &str) -> Money {
    Money::inr(s.parse::<Decimal>().unwrap())
}

#[test]
fn store_is_empty_by_default() {
    let store = JournalStore::new();
    assert_eq!(store.account_count(), 0);
    assert_eq!(store.annotation_count(), 0);
    assert_eq!(store.note_count(), 0);
    assert!(store.accounts().is_empty());
}

#[test]
fn create_and_get_account() {
    let mut store = JournalStore::new();
    let id = JournalAccountId::new("acc-1");
    let acc = store
        .create_account(
            id.clone(),
            "My Account",
            AccountProvenance::Manual,
            Currency::Inr,
        )
        .unwrap();
    assert_eq!(acc.id, id);
    assert_eq!(acc.display_name, "My Account");
    assert_eq!(store.account_count(), 1);
    assert_eq!(store.account(&id).unwrap().display_name, "My Account");
    assert_eq!(store.account(&JournalAccountId::new("nope")), None);
}

#[test]
fn create_duplicate_account_fails() {
    let mut store = JournalStore::new();
    let id = JournalAccountId::new("dup");
    store
        .create_account(id.clone(), "A", AccountProvenance::Manual, Currency::Inr)
        .unwrap();
    let err = store
        .create_account(id, "B", AccountProvenance::Manual, Currency::Inr)
        .unwrap_err();
    assert_eq!(
        err,
        StoreError::AccountAlreadyExists(JournalAccountId::new("dup"))
    );
    assert_eq!(store.account_count(), 1);
}

#[test]
fn create_account_with_balance() {
    let mut store = JournalStore::new();
    let id = JournalAccountId::new("acc-bal");
    let acc = store
        .create_account_with_balance(
            id.clone(),
            "Funded",
            AccountProvenance::Imported,
            Currency::Inr,
            money("50000.00"),
        )
        .unwrap();
    assert_eq!(acc.starting_balance, Some(money("50000.00")));
    assert_eq!(
        store.account(&id).unwrap().starting_balance,
        Some(money("50000.00"))
    );
}

#[test]
fn set_and_get_annotation() {
    let mut store = JournalStore::new();
    let id = JournalAccountId::new("acc-1");
    store
        .create_account(id.clone(), "A", AccountProvenance::Manual, Currency::Inr)
        .unwrap();

    let ann = TradeAnnotation::new()
        .with_tags(vec!["x".into()])
        .with_rating(5)
        .with_planned_risk(money("100.00"));
    store.set_annotation("trade-1", ann.clone()).unwrap();
    assert_eq!(store.annotation("trade-1"), Some(&ann));
    assert_eq!(store.annotation_count(), 1);

    assert_eq!(store.remove_annotation("trade-1"), Some(ann));
    assert_eq!(store.annotation("trade-1"), None);
    assert_eq!(store.annotation_count(), 0);
}

#[test]
fn set_note_requires_existing_account() {
    let mut store = JournalStore::new();
    let id = JournalAccountId::new("ghost");
    let err = store
        .set_note(&id, NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(), "hi")
        .unwrap_err();
    assert_eq!(
        err,
        StoreError::AccountNotFound(JournalAccountId::new("ghost"))
    );
}

#[test]
fn set_get_remove_note() {
    let mut store = JournalStore::new();
    let id = JournalAccountId::new("acc-1");
    store
        .create_account(id.clone(), "A", AccountProvenance::Manual, Currency::Inr)
        .unwrap();

    let d1 = NaiveDate::from_ymd_opt(2026, 3, 10).unwrap();
    let d2 = NaiveDate::from_ymd_opt(2026, 3, 12).unwrap();
    store.set_note(&id, d1, "first").unwrap();
    store.set_note(&id, d2, "second").unwrap();
    assert_eq!(store.note_count(), 2);
    assert_eq!(store.note(&id, d1), Some("first".to_string()));
    assert_eq!(store.note(&id, d2), Some("second".to_string()));
    assert_eq!(
        store.note(&id, NaiveDate::from_ymd_opt(2026, 3, 11).unwrap()),
        None
    );

    // Notes sorted by day.
    let notes = store.notes_for_account(&id);
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[0].day, d1);
    assert_eq!(notes[1].day, d2);

    assert_eq!(store.remove_note(&id, d1), Some("first".to_string()));
    assert_eq!(store.note_count(), 1);
    assert_eq!(store.notes_for_account(&id).len(), 1);
}

#[test]
fn pnl_convention_get_set() {
    let mut store = JournalStore::new();
    let id = JournalAccountId::new("acc-1");
    store
        .create_account(id.clone(), "A", AccountProvenance::Manual, Currency::Inr)
        .unwrap();

    assert!(JournalStore::is_supported_convention(PnlConvention::Fifo));
    store.set_pnl_convention(&id, PnlConvention::Fifo).unwrap();
    assert_eq!(
        store.account(&id).unwrap().pnl_convention,
        PnlConvention::Fifo
    );
    assert!(
        store
            .set_pnl_convention(&JournalAccountId::new("missing"), PnlConvention::Fifo)
            .is_err()
    );
}

#[test]
fn accounts_sorted_by_id() {
    let mut store = JournalStore::new();
    for id in ["b", "a", "c"] {
        store
            .create_account(
                JournalAccountId::new(id),
                format!("Acc {id}"),
                AccountProvenance::Manual,
                Currency::Inr,
            )
            .unwrap();
    }
    let ids: Vec<String> = store
        .accounts()
        .iter()
        .map(|a| a.id.as_str().to_string())
        .collect();
    assert_eq!(ids, vec!["a", "b", "c"]);
}

#[test]
fn now_is_deterministic() {
    let t = now();
    assert_eq!(t.year(), 2026);
}
