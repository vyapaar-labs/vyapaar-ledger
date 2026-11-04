//! Golden contract-note report tests.
//!
//! Pins the whole [`GoldenTestRunner`] report over the five built-in samples
//! (fixed `now`, fixed tolerance): the calculated-vs-charged fields, the
//! maximum per-trade deviation in paise, and the report's JSON round-trip.
//!
//! The sample fixtures record a broker's *reported* figures, which the
//! legacy floor-based calculator reproduces exactly for the statutory
//! fields but deviates from on `net_amount` (broker rounding conventions).
//! The pinned deviations below are therefore part of the golden contract:
//! a change in any statutory formula shows up as a different deviation.

use chrono::{DateTime, TimeZone, Utc};
use rust_decimal::Decimal;
use vyapaar_ledger_journal::{
    GoldenTestRunner, sample_total_charges_trade, sample_upstox_delivery_buy_trade,
    sample_upstox_option_buy_trade, sample_zerodha_delivery_buy_trade,
    sample_zerodha_intraday_sell_trade,
};

/// Fixed stamp injected by the tests, standing in for the deleted
/// `chrono::Utc::now()` (temporal determinism).
fn fixed_now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 2, 9, 15, 0).unwrap()
}

/// All five built-in samples at the ₹50 (5000 paise) test tolerance: the
/// source contract-note runner's own sampling tolerance.
fn full_sample_report(tolerance_paise: i64) -> vyapaar_ledger_journal::GoldenTestReport {
    GoldenTestRunner::new()
        .with_tolerance(Decimal::from(tolerance_paise))
        .add_trade(sample_zerodha_delivery_buy_trade(fixed_now()))
        .add_trade(sample_zerodha_intraday_sell_trade(fixed_now()))
        .add_trade(sample_upstox_delivery_buy_trade(fixed_now()))
        .add_trade(sample_upstox_option_buy_trade(fixed_now()))
        .add_trade(sample_total_charges_trade(fixed_now()))
        .run()
}

#[test]
fn test_golden_all_samples_pass_at_sampling_tolerance() {
    let report = full_sample_report(5000);
    assert_eq!(report.total, 5);
    assert_eq!(report.passed, 5);
    assert_eq!(report.failed, 0);
    assert_eq!(report.pass_rate(), 100.0);
    assert!(report.failed_trades().is_empty());
}

#[test]
fn test_golden_statutory_fields_match_exactly() {
    let report = full_sample_report(1);
    let mut by_id = report.results.iter().map(|r| {
        (
            r.trade_id.clone(),
            (
                r.stt_calculated,
                r.exchange_calculated,
                r.sebi_calculated,
                r.stamp_duty_calculated,
                r.gst_calculated,
                r.dp_charges_calculated,
            ),
        )
    });
    let mut field =
        |id: &str, stt: i64, exchange: i64, sebi: i64, stamp: i64, gst: i64, dp: i64| {
            let (s, e, sb, sd, g, d) = by_id.next().unwrap().1;
            assert_eq!(s, Decimal::from(stt), "{id} STT");
            assert_eq!(e, Decimal::from(exchange), "{id} exchange");
            assert_eq!(sb, Decimal::from(sebi), "{id} SEBI");
            assert_eq!(sd, Decimal::from(stamp), "{id} stamp duty");
            assert_eq!(g, Decimal::from(gst), "{id} GST");
            assert_eq!(d, Decimal::from(dp), "{id} DP charges");
        };

    // Zerodha delivery buy: 100 @ 2,50,000 paise turnover.
    field("ZERODHA_DELIVERY_BUY_001", 25_000, 25, 25, 3_750, 369, 0);
    // Zerodha intraday sell: 50 @ 75,000 paise turnover (exchange/SEBI → 7).
    field("ZERODHA_INTRADAY_SELL_001", 1_875, 7, 7, 0, 182, 0);
    // Upstox delivery buy: 200 @ 6,40,000 paise turnover.
    field("UPSTOX_DELIVERY_BUY_001", 64_000, 64, 64, 9_600, 383, 0);
    // Upstox option buy: 1 lot = 50 @ 7,500 ₹ turnover; buy side → no
    // intraday STT for options in the legacy formula... the legacy
    // `stt(false, ...)` applies the intraday rate to the BUY leg (0.025%).
    field("UPSTOX_OPTION_BUY_001", 187, 0, 0, 112, 360, 0);
    // Total-charges sample: 100 @ 2,00,000 paise turnover.
    field("TOTAL_CHARGES_TEST_001", 20_000, 20, 20, 3_000, 367, 0);
}

#[test]
fn test_golden_max_deviation_paise_is_pinned() {
    let report = full_sample_report(1);
    let mut maxes = report.results.iter().map(|r| {
        (
            r.trade_id.clone(),
            r.max_deviation_paise,
            r.net_amount_calculated,
            r.net_amount_broker,
        )
    });

    let mut case = |id: &str, deviation: &str| {
        let (trade_id, max, calc, broker) = maxes.next().unwrap();
        assert_eq!(&trade_id, id);
        assert_eq!(max.to_string(), deviation, "{id} max deviation");
        assert_ne!(calc, broker, "{id} net should deviate below tolerance 1");
    };

    case("ZERODHA_DELIVERY_BUY_001", "2150");
    case("ZERODHA_INTRADAY_SELL_001", "1165");
    case("UPSTOX_DELIVERY_BUY_001", "168");
    case("UPSTOX_OPTION_BUY_001", "299");
    case("TOTAL_CHARGES_TEST_001", "3612");
}

#[test]
fn test_golden_at_one_paise_tolerance_every_net_mismatches_but_stt_never() {
    let report = full_sample_report(1);
    assert_eq!(report.passed, 0);
    assert_eq!(report.failed, 5);
    // Broker net_amount rounding conventions make the net figure deviate
    // by more than 1 paise on every sample.
    assert_eq!(report.net_amount_mismatches().len(), 5);
    // The statutory STT formula reproduces every charged STT exactly.
    assert!(report.stt_mismatches().is_empty());
}

#[test]
fn test_golden_report_json_round_trip() {
    let report = full_sample_report(5000);
    let json = report.to_json().expect("report serializes");
    let parsed: vyapaar_ledger_journal::GoldenTestReport =
        serde_json::from_str(&json).expect("report JSON round-trips");
    assert_eq!(parsed.total, report.total);
    assert_eq!(parsed.passed, report.passed);
    assert_eq!(parsed.failed, report.failed);
    assert_eq!(parsed.tolerance_paise, report.tolerance_paise);
    assert_eq!(parsed.results.len(), report.results.len());
    for (a, b) in parsed.results.iter().zip(&report.results) {
        assert_eq!(a.trade_id, b.trade_id);
        assert_eq!(a.all_match, b.all_match);
        assert_eq!(a.max_deviation_paise, b.max_deviation_paise);
    }
}
