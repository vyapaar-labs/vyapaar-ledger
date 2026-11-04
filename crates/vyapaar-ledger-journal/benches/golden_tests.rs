//! Golden Contract-Note Validation runner.
//!
//! Simplified from the honba-kokoro CLI binary (Tier 1B zero-I/O gate): no
//! clap parsing, no filesystem reads, no CSV generation. It runs the five
//! built-in broker samples with an explicit fixed timestamp, prints the
//! report summary and its JSON, and exits non-zero if any trade deviates
//! beyond the sampling tolerance.
//!
//! Usage: cargo bench --bench golden_tests

use chrono::{DateTime, TimeZone, Utc};
use rust_decimal::Decimal;
use vyapaar_ledger_journal::{
    GoldenTestRunner, sample_total_charges_trade, sample_upstox_delivery_buy_trade,
    sample_upstox_option_buy_trade, sample_zerodha_delivery_buy_trade,
    sample_zerodha_intraday_sell_trade,
};

fn main() {
    // Fixed stamp injected instead of Utc::now() (temporal determinism):
    // the report is byte-identical on every run.
    let now: DateTime<Utc> = Utc.with_ymd_and_hms(2026, 1, 2, 9, 15, 0).unwrap();

    // 5000 paise = ₹50, the sampling tolerance the source golden fixture set
    // uses (broker net_amount rounding conventions exceed 1 paise).
    let runner = GoldenTestRunner::new()
        .with_tolerance(Decimal::from(5000))
        .add_trade(sample_zerodha_delivery_buy_trade(now))
        .add_trade(sample_zerodha_intraday_sell_trade(now))
        .add_trade(sample_upstox_delivery_buy_trade(now))
        .add_trade(sample_upstox_option_buy_trade(now))
        .add_trade(sample_total_charges_trade(now));

    let report = runner.run();
    report.print_summary();
    if let Ok(json) = report.to_json() {
        println!("{}", json);
    }

    if report.failed > 0 {
        std::process::exit(1);
    }
}
