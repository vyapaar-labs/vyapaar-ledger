//! Tests for the Phase 1 data model.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use vyapaar_core::{Currency, Money, PositionSide, Price, Quantity};

mod common;
use common::{
    annotation, annotation_with_risk, closed_trade_from_lot, instrument, journal_account,
    matched_lot, money,
};
use vyapaar_ledger_performance::model::{
    AccountProvenance, JournalAccountId, JournalNote, PnlConvention, TradeAnnotation,
};

fn exit_date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 3, 15).unwrap()
}

fn price(s: &str) -> Price {
    Price::try_new(s.parse::<Decimal>().unwrap()).unwrap()
}

#[test]
fn journal_account_defaults() {
    let acc = journal_account("acc-1");
    assert_eq!(acc.id.as_str(), "acc-1");
    assert_eq!(acc.display_name, "Test Account");
    assert_eq!(acc.provenance, AccountProvenance::Manual);
    assert_eq!(acc.currency, Currency::Inr);
    assert_eq!(acc.starting_balance, None);
    assert_eq!(acc.pnl_convention, PnlConvention::Fifo);
}

#[test]
fn journal_account_with_starting_balance() {
    let acc = journal_account("acc-2").with_starting_balance(money("100000.00"));
    assert_eq!(
        acc.starting_balance,
        Some(Money::inr(Decimal::from(100000)))
    );
    assert_eq!(acc.pnl_convention, PnlConvention::Fifo);
}

#[test]
fn trade_annotation_default() {
    let a = annotation();
    assert!(a.tags.is_empty());
    assert_eq!(a.rating, None);
    assert_eq!(a.review, None);
    assert_eq!(a.planned_risk, None);
    assert!(!a.has_risk());
}

#[test]
fn trade_annotation_builders() {
    let a = annotation()
        .with_tags(vec!["breakout".into(), "momentum".into()])
        .with_rating(4)
        .with_review("good setup")
        .with_planned_risk(money("500.00"));
    assert_eq!(a.tags, vec!["breakout", "momentum"]);
    assert_eq!(a.rating, Some(4));
    assert_eq!(a.review, Some("good setup".to_string()));
    assert_eq!(a.planned_risk, Some(money("500.00")));
    assert!(a.has_risk());
}

#[test]
fn trade_annotation_planned_risk_zero_means_no_risk() {
    let a = annotation_with_risk("0.00");
    assert!(!a.has_risk());
}

#[test]
fn journal_note_creation() {
    let note = JournalNote::new(
        JournalAccountId::new("acc-1"),
        NaiveDate::from_ymd_opt(2026, 3, 15).unwrap(),
        "Nice day",
    );
    assert_eq!(note.account_id.as_str(), "acc-1");
    assert_eq!(note.day, NaiveDate::from_ymd_opt(2026, 3, 15).unwrap());
    assert_eq!(note.text, "Nice day");
}

#[test]
fn closed_trade_from_matched_lot_maps_fields() {
    let lot = matched_lot("LOT-1", "2500.00", "2600.00", 10, 45, false);
    let trade = closed_trade_from_lot(&lot, exit_date());

    assert_eq!(trade.trade_id, "LOT-1");
    assert_eq!(trade.instrument, instrument("RELIANCE"));
    assert_eq!(trade.side, PositionSide::Long);
    assert_eq!(trade.quantity, Quantity::from_u64(10));
    assert_eq!(trade.entry_price, price("2500.00"));
    assert_eq!(trade.exit_price, price("2600.00"));
    // realized_pnl = (2600 - 2500) * 10 = 1000
    assert_eq!(trade.realized_pnl, money("1000.00"));
    assert_eq!(trade.holding_period_days, 45);
    assert!(!trade.is_long_term);
    assert_eq!(trade.exit_date, exit_date());
    assert_eq!(trade.duration_ms, None);
    assert!(trade.annotation.tags.is_empty());
    assert_eq!(trade.fees, money("0.00"));
}

#[test]
fn closed_trade_net_pnl_subtracts_fees() {
    let lot = matched_lot("LOT-2", "100.00", "110.00", 5, 10, false);
    let mut trade = closed_trade_from_lot(&lot, exit_date());
    // realized = (110-100)*5 = 50; fees = 20; net = 30
    trade.fees = money("20.00");
    assert_eq!(trade.net_pnl(), money("30.00"));
    assert!(trade.is_win());
    assert!(!trade.is_loss());
    assert!(!trade.is_breakeven());
    assert_eq!(trade.gross_profit(), money("30.00"));
    assert_eq!(trade.gross_loss(), money("0.00"));
}

#[test]
fn closed_trade_loss_and_breakeven() {
    // Loss: exit < entry
    let lot = matched_lot("LOT-3", "100.00", "90.00", 4, 20, false);
    let mut trade = closed_trade_from_lot(&lot, exit_date());
    trade.fees = money("5.00");
    // realized = -40; net = -45
    assert!(trade.is_loss());
    assert!(!trade.is_win());
    assert!(!trade.is_breakeven());
    assert_eq!(trade.net_pnl(), money("-45.00"));
    assert_eq!(trade.gross_profit(), money("0.00"));
    assert_eq!(trade.gross_loss(), money("45.00"));

    // Breakeven
    let lot2 = matched_lot("LOT-4", "100.00", "100.00", 4, 20, false);
    let trade2 = closed_trade_from_lot(&lot2, exit_date());
    assert!(trade2.is_breakeven());
    assert!(!trade2.is_win());
    assert!(!trade2.is_loss());
    assert_eq!(trade2.net_pnl(), money("0.00"));
}

#[test]
fn closed_trade_realized_r() {
    let lot = matched_lot("LOT-5", "100.00", "110.00", 10, 5, false);
    let mut trade = closed_trade_from_lot(&lot, exit_date());
    // no planned risk -> None
    assert_eq!(trade.realized_r(), None);

    // planned risk 100; net = (110-100)*10 = 100; R = 100/100 = 1
    trade.annotation = annotation_with_risk("100.00");
    assert_eq!(trade.realized_r(), Some(Decimal::ONE));

    // planned risk zero -> None
    trade.annotation = TradeAnnotation::new().with_planned_risk(money("0.00"));
    assert_eq!(trade.realized_r(), None);
}
