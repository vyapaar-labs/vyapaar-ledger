//! Journal unit tests.
//!
//! Ported from the `honba-kokoro-ledger` inline `#[cfg(test)]` blocks
//! (`src/lib.rs` lines 612–864 and `src/golden_tests/mod.rs`, migration
//! guide Fix 4). Rewritten for the canonical API: every test uses a fixed
//! timestamp (temporal determinism), the chart of accounts is keyed by
//! [`AccountCode`] (the duplicated legacy account names are gone), and the
//! contract-note samples inject `now` explicitly.

use chrono::{DateTime, TimeZone, Utc};
use rust_decimal::Decimal;
use std::collections::BTreeMap;
use vyapaar_ledger_journal::{
    AccountCode, GoldenTestRunner, TaxCalculator, Transaction, post_transaction,
    sample_zerodha_delivery_buy_trade,
};

/// Fixed stamp injected by the tests, standing in for the deleted
/// `chrono::Utc::now()` (temporal determinism).
fn fixed_now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 2, 9, 15, 0).unwrap()
}

#[test]
fn test_post_transaction_balanced() {
    let tx = Transaction {
        id: "tx_001".to_string(),
        timestamp: fixed_now(),
        debits: BTreeMap::from([(
            AccountCode::new(AccountCode::CASH_AT_BROKER),
            Decimal::from(1000),
        )]),
        credits: BTreeMap::from([(
            AccountCode::new(AccountCode::INCOME_REALIZED_PNL),
            Decimal::from(1000),
        )]),
        description: "Test transaction".to_string(),
    };
    assert!(post_transaction(&tx).is_ok());
}

#[test]
fn test_post_transaction_unbalanced() {
    let tx = Transaction {
        id: "tx_002".to_string(),
        timestamp: fixed_now(),
        debits: BTreeMap::from([(
            AccountCode::new(AccountCode::CASH_AT_BROKER),
            Decimal::from(1000),
        )]),
        credits: BTreeMap::from([(
            AccountCode::new(AccountCode::INCOME_REALIZED_PNL),
            Decimal::from(500),
        )]),
        description: "Unbalanced transaction".to_string(),
    };
    assert!(post_transaction(&tx).is_err());
}

#[test]
fn test_stt_delivery_buy() {
    let stt = TaxCalculator::stt(true, Decimal::from(2000), Decimal::from(10));
    assert_eq!(stt, Decimal::from(20));
}

#[test]
fn test_stt_intraday_sell() {
    let stt = TaxCalculator::stt(false, Decimal::from(2000), Decimal::from(10));
    assert_eq!(stt, Decimal::from(5));
}

#[test]
#[allow(non_snake_case)]
fn test_GST() {
    let gst = TaxCalculator::GST(Decimal::from(1000), Decimal::from(200), Decimal::from(50));
    assert_eq!(gst, Decimal::from(225));
}

#[test]
fn test_stamp_duty_buy() {
    let sd = TaxCalculator::stamp_duty(Decimal::from(2000), Decimal::from(10), true);
    assert_eq!(sd, Decimal::from(3));
}

#[test]
fn test_sebi_turnover_charges() {
    let sebi = TaxCalculator::sebi_turnover_charges(Decimal::from(10_000_000));
    assert_eq!(sebi, Decimal::from(10));
}

#[test]
fn test_dp_charges_delivery_sale() {
    let dp = TaxCalculator::dp_charges(true);
    // ₹13.50 + 18% GST ≥ ₹13.50
    assert!(dp >= Decimal::from(1350) / Decimal::from(100));
}

#[test]
fn test_dp_charges_non_delivery() {
    let dp = TaxCalculator::dp_charges(false);
    assert_eq!(dp, Decimal::from(0));
}

#[test]
fn test_transaction_balance() {
    let tx = Transaction {
        id: "tx_test".to_string(),
        timestamp: fixed_now(),
        debits: BTreeMap::from([(
            AccountCode::new(AccountCode::CASH_AT_BROKER),
            Decimal::from(500),
        )]),
        credits: BTreeMap::from([(
            AccountCode::new(AccountCode::INCOME_REALIZED_PNL),
            Decimal::from(500),
        )]),
        description: "Test transaction".to_string(),
    };
    assert!(post_transaction(&tx).is_ok());
}

// ————————————————————————————————————————
// Broker Contract Note Validation Tests (Phase 5.2)
// ————————————————————————————————————————

/// Zerodha contract note sample for a delivery buy trade
/// Based on real Zerodha contract note format
#[test]
fn test_zerodha_contract_note_delivery_buy() {
    // Trade: Buy 100 RELIANCE @ ₹2,500.00 (Delivery)
    let price = Decimal::from(250000); // ₹2,500.00 in paise
    let quantity = Decimal::from(100);
    let brokerage = Decimal::from(2000); // ₹20.00

    // Calculate all charges using our TaxCalculator
    let stt = TaxCalculator::stt(true, price, quantity);
    let turnover = price * quantity;
    let exchange_charges = TaxCalculator::sebi_turnover_charges(turnover); // Using SEBI rate for exchange
    let sebi_charges = TaxCalculator::sebi_turnover_charges(turnover);
    let stamp_duty = TaxCalculator::stamp_duty(price, quantity, true);
    let gst = TaxCalculator::GST(brokerage, exchange_charges, sebi_charges);

    // Expected values from Zerodha contract note (in paise):
    // Trade: 100 shares @ ₹2,500 = ₹2,50,000 turnover
    // STT: 0.1% of ₹2,50,000 = ₹250 = 25,000 paise
    // Exchange charges: ₹2.97 per crore = ~₹0.07
    // SEBI charges: ₹10 per crore = ₹0.25
    // Stamp duty: 0.015% of ₹2,50,000 = ₹37.50 = 3,750 paise
    // GST: 18% on (2000 + exchange + sebi)

    assert_eq!(stt, Decimal::from(25000)); // 0.1% of ₹2,50,000 = ₹250 = 25,000 paise
    assert_eq!(sebi_charges, Decimal::from(25)); // ₹10 per crore on ₹2.5 lakh = ₹0.25 = 25 paise
    assert_eq!(stamp_duty, Decimal::from(3750)); // 0.015% of ₹2,50,000 = ₹37.50 = 3,750 paise
    assert_eq!(gst, Decimal::from(369)); // 18% of (2000 + ~0 + ~0) ≈ 360
}

/// Zerodha contract note sample for an intraday sell trade
#[test]
fn test_zerodha_contract_note_intraday_sell() {
    // Trade: Sell 50 INFY @ ₹1,500.00 (Intraday)
    let price = Decimal::from(150000); // ₹1,500.00 in paise
    let quantity = Decimal::from(50);
    let brokerage = Decimal::from(1000); // ₹10.00

    let stt = TaxCalculator::stt(false, price, quantity);
    let turnover = price * quantity;
    let exchange_charges = TaxCalculator::sebi_turnover_charges(turnover);
    let sebi_charges = TaxCalculator::sebi_turnover_charges(turnover);
    let stamp_duty = TaxCalculator::stamp_duty(price, quantity, false); // No stamp duty on sell
    let gst = TaxCalculator::GST(brokerage, exchange_charges, sebi_charges);

    // Trade: 50 shares @ ₹1,500 = ₹75,000 turnover
    // STT: 0.025% of ₹75,000 = ₹18.75 = 1,875 paise
    // Exchange charges: ₹2.97 per crore
    // SEBI charges: ₹10 per crore
    // Stamp duty: 0 on sell
    // GST: 18% on (1000 + exchange + sebi)

    assert_eq!(stt, Decimal::from(1875)); // 0.025% of ₹75,000 = ₹18.75 = 1,875 paise
    assert_eq!(stamp_duty, Decimal::from(0)); // No stamp duty on sell
    assert_eq!(gst, Decimal::from(182)); // 18% of 1000
}

/// Upstox contract note sample for a delivery buy trade
#[test]
fn test_upstox_contract_note_delivery_buy() {
    // Trade: Buy 200 TCS @ ₹3,200.00 (Delivery)
    let price = Decimal::from(320000); // ₹3,200.00 in paise
    let quantity = Decimal::from(200);
    let brokerage = Decimal::from(2000); // ₹20.00 (Upstox charges ₹20 per order)

    let stt = TaxCalculator::stt(true, price, quantity);
    let turnover = price * quantity;
    let exchange_charges = TaxCalculator::sebi_turnover_charges(turnover);
    let sebi_charges = TaxCalculator::sebi_turnover_charges(turnover);
    let stamp_duty = TaxCalculator::stamp_duty(price, quantity, true);
    let _gst = TaxCalculator::GST(brokerage, exchange_charges, sebi_charges);

    // Trade: 200 shares @ ₹3,200 = ₹6,40,000 turnover
    // STT: 0.1% of ₹6,40,000 = ₹640 = 64,000 paise
    // Exchange charges: NSE ₹2.97 per crore
    // SEBI charges: ₹10 per crore
    // Stamp duty: 0.015% of ₹6,40,000 = ₹96 = 9,600 paise
    // GST: 18% on (2000 + exchange + sebi)

    assert_eq!(stt, Decimal::from(64000)); // 0.1% of ₹6,40,000 = ₹640 = 64,000 paise
    assert_eq!(stamp_duty, Decimal::from(9600)); // 0.015% of ₹6,40,000 = ₹96 = 9,600 paise
}

/// Upstox contract note for F&O trade (option buy)
#[test]
fn test_upstox_contract_note_option_buy() {
    // Trade: Buy 1 lot NIFTY 19000 CE @ ₹150.00 (Option Premium)
    // Lot size: 50
    let price = Decimal::from(15000); // ₹150.00 in paise
    let quantity = Decimal::from(50); // 1 lot
    let _brokerage = Decimal::from(2000); // ₹20.00 per order

    // For options: STT is 0.0125% on premium (on sell side)
    // On buy side: No STT
    // Exchange charges: NSE ₹2.97 per crore on premium
    // SEBI charges: ₹10 per crore on premium
    // Stamp duty: 0.003% on buy side for options
    // GST: 18% on (brokerage + exchange + sebi)

    let stt = TaxCalculator::stt(false, price, quantity); // Intraday rate for options
    let turnover = price * quantity;
    let _exchange_charges = TaxCalculator::sebi_turnover_charges(turnover);
    let _sebi_charges = TaxCalculator::sebi_turnover_charges(turnover);

    // Turnover = 15000 * 50 = 7,50,000 paise = ₹7,500
    // Exchange: ₹2.97 per crore = 0.22 paise
    // SEBI: ₹10 per crore = 0.75 paise

    // Using intraday rate as approximation for options
    assert_eq!(stt, Decimal::from(187)); // 0.025% of ₹7,500 = ₹18.75 = 1,875 paise
}

/// Validate total charges match broker contract note within tolerance
#[test]
fn test_total_charges_match_broker_note() {
    // Test case: Delivery buy 100 shares @ ₹2000
    let price = Decimal::from(200000);
    let quantity = Decimal::from(100);
    let brokerage = Decimal::from(2000);

    let stt = TaxCalculator::stt(true, price, quantity);
    let turnover = price * quantity;
    let exchange = TaxCalculator::sebi_turnover_charges(turnover);
    let sebi = TaxCalculator::sebi_turnover_charges(turnover);
    let stamp = TaxCalculator::stamp_duty(price, quantity, true);
    let gst = TaxCalculator::GST(Decimal::from(2000), exchange, stt);
    let dp = TaxCalculator::dp_charges(true);

    let total_charges = brokerage + stt + exchange + sebi + stamp + gst + dp;

    // Net amount = price * quantity + total_charges (for buy)
    let net_amount = (price * quantity) + total_charges;

    // For buy 100 @ 2000: 2,00,000 + charges
    // Expected net: ~2,00,000 + 2,000 (brokerage) + 200 (STT) + 2 (exch) + 2 (SEBI) + 30 (stamp) + 360 (GST) + 1593 (DP)
    // = ~2,04,217 paise = ₹2,042.17

    let expected_net = Decimal::from(20029019); // Approximate expected net in paise
    let tolerance = Decimal::from(100); // ±₹1 tolerance

    assert!(
        (net_amount - expected_net).abs() <= tolerance,
        "Net amount {} should be within ₹1 of expected {}",
        net_amount,
        expected_net
    );
}

// ————————————————————————————————————————
// Golden Runner Tests (ported from golden_tests/mod.rs)
// ————————————————————————————————————————

#[test]
fn test_golden_runner_basic() {
    let runner = GoldenTestRunner::new()
        .with_tolerance(Decimal::from(5000)) // 5000 paise = ₹50 tolerance for test data
        .add_trade(sample_zerodha_delivery_buy_trade(fixed_now()));

    let report = runner.run();
    assert_eq!(report.total, 1);
    assert_eq!(report.passed, 1);
    assert_eq!(report.failed, 0);
}

#[test]
fn test_csv_record_into_trade_maps_uppercase_fields() {
    use vyapaar_core::OrderSide;
    use vyapaar_ledger_journal::{CsvRecord, ProductType};

    let make_record = |side: &str, product_type: &str| CsvRecord {
        trade_id: "TRD00000001".to_string(),
        trade_date: "2026-01-02T09:15:00+00:00".to_string(),
        symbol: "RELIANCE".to_string(),
        exchange: "NSE".to_string(),
        side: side.to_string(), // case-insensitive like the real notes
        quantity: Decimal::from(100),
        price: Decimal::from(250000),
        product_type: product_type.to_string(),
        brokerage: Decimal::from(2000),
        stt: Decimal::from(25000),
        exchange_charges: Decimal::from(25),
        sebi_charges: Decimal::from(25),
        stamp_duty: Decimal::from(3750),
        gst: Decimal::from(369),
        dp_charges: Decimal::from(0),
        net_amount: Decimal::from(25029019),
        settle_date: "2026-01-05".to_string(),
    };

    let trade = make_record("buy", "cnc").into_trade("zerodha").unwrap();
    assert_eq!(trade.broker, "zerodha");
    assert_eq!(trade.side, OrderSide::Buy);
    assert_eq!(trade.product_type, ProductType::Delivery);
    assert_eq!(
        trade.trade_time,
        DateTime::parse_from_rfc3339("2026-01-02T09:15:00+00:00")
            .unwrap()
            .with_timezone(&Utc)
    );

    let mis_fill = make_record("sell", "mis").into_trade("zerodha").unwrap();
    assert_eq!(mis_fill.side, OrderSide::Sell);
    assert_eq!(mis_fill.product_type, ProductType::Intraday);

    let invalid_side = make_record("hold", "delivery").into_trade("zerodha");
    assert!(invalid_side.is_err());
    let invalid_product = make_record("buy", "weird").into_trade("zerodha");
    assert!(invalid_product.is_err());
}

#[test]
fn test_generate_sample_csv_is_deterministic() {
    use vyapaar_ledger_journal::generate_sample_csv;

    let a = generate_sample_csv("zerodha", 3, fixed_now());
    let b = generate_sample_csv("zerodha", 3, fixed_now());
    assert_eq!(a, b); // same now → byte-identical fixture (temporal determinism)

    let header = a.lines().next().unwrap();
    assert!(header.starts_with("trade_id,trade_date,symbol"));
    assert_eq!(a.lines().count(), 4); // header + 3 rows
}
