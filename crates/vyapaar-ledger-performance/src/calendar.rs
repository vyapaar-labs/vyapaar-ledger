//! Phase 3 — daily P&L calendar aggregation.
//!
//! Groups closed trades by exit date into per-day buckets: net PnL, gross
//! PnL, fees, trade count, wins/losses/breakevens, and volume — mirroring
//! Vela's `/api/journal/calendar?month=YYYY-MM` response.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::Money;
use crate::model::ClosedTrade;

/// One day's PnL bucket in the calendar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DailyPnl {
    pub date: NaiveDate,
    pub net_pnl: Money,
    pub gross_pnl: Money,
    pub fees: Money,
    pub trades: u64,
    pub wins: u64,
    pub losses: u64,
    pub breakevens: u64,
    pub volume: Money,
}

impl DailyPnl {
    pub fn new(date: NaiveDate) -> Self {
        Self {
            date,
            net_pnl: Money::zero_inr(),
            gross_pnl: Money::zero_inr(),
            fees: Money::zero_inr(),
            trades: 0,
            wins: 0,
            losses: 0,
            breakevens: 0,
            volume: Money::zero_inr(),
        }
    }
}

/// Aggregated calendar result for a month.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PnlCalendar {
    pub year: i32,
    pub month: u32,
    pub weeks: Vec<CalendarWeek>,
}

/// One week within the calendar: a row of up to 7 days.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalendarWeek {
    pub days: Vec<DailyPnl>,
}

/// Aggregate closed trades into per-day buckets, then lay them out into
/// weeks for the given month (Vela calendar shape). Days outside the month
/// that fall inside the month's first/last week are included as empty buckets
/// so the UI can render a full weekly grid.
pub fn build_calendar(trades: &[ClosedTrade], year: i32, month: u32) -> PnlCalendar {
    let mut by_day: BTreeMap<NaiveDate, DailyPnl> = BTreeMap::new();

    // Month boundaries: [first, last] inclusive.
    let first = NaiveDate::from_ymd_opt(year, month, 1).expect("valid month");
    let next_first = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1).expect("valid month")
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1).expect("valid month")
    };
    let last = next_first.pred_opt().expect("valid month");

    for trade in trades {
        if trade.exit_date < first || trade.exit_date > last {
            continue;
        }
        let bucket = by_day
            .entry(trade.exit_date)
            .or_insert_with(|| DailyPnl::new(trade.exit_date));
        let net = trade.net_pnl().amount;
        bucket.net_pnl = Money::inr(bucket.net_pnl.amount + net);
        bucket.fees = Money::inr(bucket.fees.amount + trade.fees.amount);
        bucket.trades += 1;
        bucket.volume =
            Money::inr(bucket.volume.amount + trade.exit_price.value() * trade.quantity.value());
        if trade.is_win() {
            bucket.wins += 1;
            bucket.gross_pnl = Money::inr(bucket.gross_pnl.amount + net);
        } else if trade.is_loss() {
            bucket.losses += 1;
        } else {
            bucket.breakevens += 1;
        }
    }

    // Build the weekly grid: chunk every 7 days starting from the 1st of the
    // month. Leading days of the first partial week and trailing days of the
    // last partial week are included as empty buckets so the UI can render a
    // full weekly grid.
    let mut weeks = Vec::new();
    let mut current_week: Vec<DailyPnl> = Vec::new();
    let mut cursor = first;
    let mut day_index = 0u64;
    while cursor <= last {
        current_week.push(
            by_day
                .get(&cursor)
                .cloned()
                .unwrap_or_else(|| DailyPnl::new(cursor)),
        );
        day_index += 1;
        if day_index.is_multiple_of(7) {
            weeks.push(CalendarWeek {
                days: std::mem::take(&mut current_week),
            });
        }
        cursor = cursor.succ_opt().expect("valid date");
    }
    // Flush any partial final week.
    if !current_week.is_empty() {
        weeks.push(CalendarWeek { days: current_week });
    }

    PnlCalendar { year, month, weeks }
}

/// Aggregate closed trades into per-day buckets (no weekly layout).
pub fn daily_pnl_buckets(trades: &[ClosedTrade]) -> BTreeMap<NaiveDate, DailyPnl> {
    let mut by_day: BTreeMap<NaiveDate, DailyPnl> = BTreeMap::new();
    for trade in trades {
        let bucket = by_day
            .entry(trade.exit_date)
            .or_insert_with(|| DailyPnl::new(trade.exit_date));
        let net = trade.net_pnl().amount;
        bucket.net_pnl = Money::inr(bucket.net_pnl.amount + net);
        bucket.fees = Money::inr(bucket.fees.amount + trade.fees.amount);
        bucket.trades += 1;
        bucket.volume =
            Money::inr(bucket.volume.amount + trade.exit_price.value() * trade.quantity.value());
        if trade.is_win() {
            bucket.wins += 1;
            bucket.gross_pnl = Money::inr(bucket.gross_pnl.amount + net);
        } else if trade.is_loss() {
            bucket.losses += 1;
        } else {
            bucket.breakevens += 1;
        }
    }
    by_day
}

/// Sum of fees across all trades (a Money total).
pub fn total_fees(trades: &[ClosedTrade]) -> Money {
    let mut total = Decimal::ZERO;
    for trade in trades {
        total += trade.fees.amount;
    }
    Money::inr(total)
}
