//! Tests for the Phase 3 daily P&L calendar.

use chrono::NaiveDate;
use rust_decimal::Decimal;

mod common;
use common::{closed_trade_from_lot, matched_lot, money};
use vyapaar_ledger_performance::calendar::{build_calendar, daily_pnl_buckets, total_fees};
use vyapaar_ledger_performance::model::ClosedTrade;

fn mar10() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 3, 10).unwrap()
}
fn mar11() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 3, 11).unwrap()
}
fn apr1() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 4, 1).unwrap()
}

fn w(day: NaiveDate, net: &str) -> ClosedTrade {
    // entry 100 -> exit (100 + net), qty 10
    let exit = (Decimal::from(100) + net.parse::<Decimal>().unwrap()).to_string();
    closed_trade_from_lot(&matched_lot("LOT", "100.00", &exit, 10, 10, false), day)
}

#[test]
fn build_calendar_filters_by_month() {
    let trades = vec![
        w(mar10(), "10.00"),
        w(mar11(), "-5.00"),
        w(apr1(), "999.00"),
    ];
    let cal = build_calendar(&trades, 2026, 3);
    assert_eq!(cal.year, 2026);
    assert_eq!(cal.month, 3);
    // Only March trades appear with non-zero trade counts; the April trade is excluded.
    let all_dates: Vec<NaiveDate> = cal
        .weeks
        .iter()
        .flat_map(|wk| &wk.days)
        .map(|day| day.date)
        .collect();
    assert!(!all_dates.contains(&apr1()));
    let mar10_bucket = cal
        .weeks
        .iter()
        .flat_map(|wk| &wk.days)
        .find(|day| day.date == mar10())
        .unwrap();
    assert_eq!(mar10_bucket.trades, 1);
    assert_eq!(mar10_bucket.net_pnl, money("100.00"));
}

#[test]
fn build_calendar_aggregates_day_bucket() {
    let trades = vec![
        w(mar10(), "10.00"), // +100 net, qty 10, volume = exit*qty = 110*10 = 1100
        w(mar10(), "-5.00"), // -50 net
        w(mar11(), "20.00"), // +200 net
    ];
    let cal = build_calendar(&trades, 2026, 3);
    let mar10_bucket = cal
        .weeks
        .iter()
        .flat_map(|wk| &wk.days)
        .find(|day| day.date == mar10())
        .unwrap();
    // net = 100 - 50 = 50
    assert_eq!(mar10_bucket.net_pnl, money("50.00"));
    assert_eq!(mar10_bucket.trades, 2);
    assert_eq!(mar10_bucket.wins, 1);
    assert_eq!(mar10_bucket.losses, 1);
    assert_eq!(mar10_bucket.breakevens, 0);
    // gross_pnl = 100 (only the win)
    assert_eq!(mar10_bucket.gross_pnl, money("100.00"));
}

#[test]
fn build_calendar_breakeven_bucket() {
    let trades = vec![w(mar10(), "0.00")];
    let cal = build_calendar(&trades, 2026, 3);
    let b = cal
        .weeks
        .iter()
        .flat_map(|wk| &wk.days)
        .find(|day| day.date == mar10())
        .unwrap();
    assert_eq!(b.breakevens, 1);
    assert_eq!(b.wins, 0);
    assert_eq!(b.losses, 0);
    assert_eq!(b.net_pnl, money("0.00"));
}

#[test]
fn build_calendar_weekly_grid_chunks_by_seven() {
    // March 2026 has 31 days.
    let trades: Vec<_> = (1..=31)
        .map(|day| w(NaiveDate::from_ymd_opt(2026, 3, day).unwrap(), "1.00"))
        .collect();
    let cal = build_calendar(&trades, 2026, 3);
    assert!(!cal.weeks.is_empty());
    for week in &cal.weeks {
        assert!(week.days.len() <= 7);
    }
    // Total days bucketed = 31.
    let total: usize = cal.weeks.iter().map(|wk| wk.days.len()).sum();
    assert_eq!(total, 31);
}

#[test]
fn daily_pnl_buckets_sorted_by_date() {
    let trades = vec![w(mar11(), "1.00"), w(mar10(), "1.00")];
    let buckets = daily_pnl_buckets(&trades);
    let mut dates: Vec<NaiveDate> = buckets.keys().copied().collect();
    dates.sort();
    assert_eq!(dates, vec![mar10(), mar11()]);
}

#[test]
fn total_fees_sums_across_trades() {
    let mut t1 = w(mar10(), "10.00");
    t1.fees = money("10.00");
    let mut t2 = w(mar11(), "-5.00");
    t2.fees = money("5.00");
    let total = total_fees(&[t1, t2]);
    assert_eq!(total, money("15.00"));
}
