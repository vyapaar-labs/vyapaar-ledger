//! Tests for the Phase 3 account comparison.

use chrono::{Datelike, NaiveDate};
use vyapaar_core::Money;

mod common;
use common::{loss_trade, now, win_trade};
use vyapaar_ledger_performance::compare::{compare_account_vs_prior, compare_accounts};
use vyapaar_ledger_performance::model::JournalAccountId;

#[test]
fn compare_accounts_two_sides() {
    let a_id = JournalAccountId::new("acc-a");
    let b_id = JournalAccountId::new("acc-b");
    let trades_a = vec![win_trade(
        "A1",
        NaiveDate::from_ymd_opt(2026, 3, 10).unwrap(),
    )];
    let trades_b = vec![loss_trade(
        "B1",
        NaiveDate::from_ymd_opt(2026, 3, 10).unwrap(),
    )];

    let comp = compare_accounts(a_id.clone(), trades_a, b_id.clone(), trades_b, now(), false);

    assert_eq!(comp.account_a.account_id, a_id);
    assert_eq!(comp.account_b.account_id, b_id);
    assert!(!comp.is_prior_period);
    assert_eq!(comp.account_a.metrics.wins, 1);
    assert_eq!(comp.account_a.metrics.net_pnl, money("100.00"));
    assert_eq!(comp.account_b.metrics.losses, 1);
    assert_eq!(comp.account_b.metrics.net_pnl, money("-100.00"));
}

#[test]
fn compare_account_vs_prior_splits_at_boundary() {
    let id = JournalAccountId::new("acc-1");
    // Trades before boundary (prior) and after (current).
    let trades = vec![
        win_trade("P1", NaiveDate::from_ymd_opt(2026, 2, 1).unwrap()), // prior
        win_trade("P2", NaiveDate::from_ymd_opt(2026, 2, 15).unwrap()), // prior
        loss_trade("C1", NaiveDate::from_ymd_opt(2026, 3, 10).unwrap()), // current
        win_trade("C2", NaiveDate::from_ymd_opt(2026, 3, 20).unwrap()), // current
    ];
    let boundary = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();

    let comp = compare_account_vs_prior(id.clone(), trades, boundary, now());

    assert!(comp.is_prior_period);
    assert_eq!(comp.account_a.account_id, id); // current
    assert_eq!(comp.account_b.account_id, id); // prior
    // Current: one loss (-100), one win (+100) → net 0
    assert_eq!(comp.account_a.metrics.wins, 1);
    assert_eq!(comp.account_a.metrics.losses, 1);
    assert_eq!(comp.account_a.metrics.net_pnl, money("0.00"));
    // Prior: two wins → net +200
    assert_eq!(comp.account_b.metrics.wins, 2);
    assert_eq!(comp.account_b.metrics.net_pnl, money("200.00"));
}

#[test]
fn compare_empty_both_sides() {
    let a_id = JournalAccountId::new("a");
    let b_id = JournalAccountId::new("b");
    let comp = compare_accounts(a_id, vec![], b_id, vec![], now(), true);
    assert_eq!(comp.account_a.metrics.total_trades, 0);
    assert_eq!(comp.account_b.metrics.total_trades, 0);
    assert!(comp.account_a.edge_score.score.is_none());
    assert!(comp.account_b.edge_score.score.is_none());
}

fn money(s: &str) -> Money {
    Money::inr(s.parse::<rust_decimal::Decimal>().unwrap())
}

#[test]
fn now_is_deterministic() {
    let t = now();
    assert_eq!(t.year(), 2026);
}
