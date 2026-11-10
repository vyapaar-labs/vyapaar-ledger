//! Shared test helpers for `vyapaar-ledger-performance`.
#![allow(dead_code)]

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use rust_decimal::Decimal;
use vyapaar_core::{Currency, Exchange, InstrumentId, Money, PositionSide, Price, Quantity};

use vyapaar_ledger_performance::model::{
    AccountProvenance, ClosedTrade, JournalAccount, JournalAccountId, TradeAnnotation,
};
use vyapaar_ledger_portfolio::MatchedLot;

pub fn instrument(symbol: &str) -> InstrumentId {
    InstrumentId::new(Exchange::Nse, symbol)
}

pub fn price(s: &str) -> Price {
    Price::try_new(s.parse::<Decimal>().unwrap()).unwrap()
}

pub fn qty(n: u64) -> Quantity {
    Quantity::from_u64(n)
}

pub fn money(s: &str) -> Money {
    Money::inr(s.parse::<Decimal>().unwrap())
}

pub fn d(s: &str) -> Decimal {
    s.parse::<Decimal>().unwrap()
}

pub fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()
}

pub fn matched_lot(
    lot_id: &str,
    entry: &str,
    exit: &str,
    qty: u64,
    holding_days: i64,
    long_term: bool,
) -> MatchedLot {
    let entry_p = price(entry);
    let exit_p = price(exit);
    let q = Quantity::from_u64(qty);
    let realized = Money::inr((exit_p.value() - entry_p.value()) * q.value());
    MatchedLot {
        lot_id: lot_id.to_string(),
        matched_quantity: q,
        entry_price: entry_p,
        exit_price: exit_p,
        realized_pnl: realized,
        holding_period_days: holding_days,
        is_long_term: long_term,
    }
}

pub fn annotation() -> TradeAnnotation {
    TradeAnnotation::new()
}

pub fn annotation_with_risk(risk: &str) -> TradeAnnotation {
    TradeAnnotation::new().with_planned_risk(money(risk))
}

pub fn closed_trade_from_lot(lot: &MatchedLot, exit_date: NaiveDate) -> ClosedTrade {
    ClosedTrade::from_matched_lot(
        lot,
        instrument("RELIANCE"),
        PositionSide::Long,
        exit_date,
        annotation(),
    )
}

pub fn journal_account(id: &str) -> JournalAccount {
    JournalAccount::new(
        JournalAccountId::new(id.to_string()),
        "Test Account",
        AccountProvenance::Manual,
        Currency::Inr,
    )
}

pub fn win_trade(lot_id: &str, day: NaiveDate) -> ClosedTrade {
    // entry 100 -> exit 110, qty 10: realized +100
    closed_trade_from_lot(&matched_lot(lot_id, "100.00", "110.00", 10, 10, false), day)
}

pub fn loss_trade(lot_id: &str, day: NaiveDate) -> ClosedTrade {
    // entry 100 -> exit 90, qty 10: realized -100
    closed_trade_from_lot(&matched_lot(lot_id, "100.00", "90.00", 10, 10, false), day)
}

pub fn breakeven_trade(lot_id: &str, day: NaiveDate) -> ClosedTrade {
    closed_trade_from_lot(&matched_lot(lot_id, "100.00", "100.00", 10, 10, false), day)
}
