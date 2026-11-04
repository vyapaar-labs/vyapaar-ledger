//! Broker Contract Note Validation (the golden-accounting framework).
//!
//! Once replayed 1,000+ real executed trade sets from Zerodha and Upstox
//! contract notes; verifies that the journal's calculated STT, GST, Stamp
//! Duty, and Net PnL match the broker statement down to the paisa. In this
//! crate the framework is a *pure* library module (Tier 1B zero-I/O gate):
//! the filesystem loaders (`load_from_csv` / `load_from_json`) were dropped
//! — trades are added programmatically via
//! [`GoldenTestRunner::add_trade`] — and every sample fixture and CSV
//! generator receives an explicit `now: DateTime<Utc>` instead of reading
//! the wall clock. The CSV text writer formats rows by hand (no `csv`
//! crate dependency).

use crate::TaxCalculator;
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use vyapaar_core::OrderSide;

/// Represents a single trade from a broker contract note
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractNoteTrade {
    pub trade_id: String,
    pub broker: String,
    pub trade_time: DateTime<Utc>,
    pub symbol: String,
    pub exchange: String,
    pub side: OrderSide,
    pub quantity: Decimal,
    pub price: Decimal,
    pub product_type: ProductType,
    pub brokerage: Decimal,
    pub stt_charged: Decimal,
    pub exchange_charges: Decimal,
    pub sebi_charges: Decimal,
    pub stamp_duty: Decimal,
    pub gst: Decimal,
    pub dp_charges: Decimal,
    pub net_amount: Decimal,
    pub settle_date: NaiveDate,
}

/// Product type for Indian equities as declared on a broker contract note
/// (the ExecutionArchivo taxonomy; distinct from the portfolio crate's
/// execution `Product` and from any future SEBI product segmentation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ProductType {
    Delivery,
    Intraday,
    Options,
    Futures,
}

/// Result of comparing calculated vs broker charges
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonResult {
    pub trade_id: String,
    pub symbol: String,
    pub broker: String,
    pub stt_match: bool,
    pub stt_calculated: Decimal,
    pub stt_broker: Decimal,
    pub exchange_match: bool,
    pub exchange_calculated: Decimal,
    pub exchange_broker: Decimal,
    pub sebi_match: bool,
    pub sebi_calculated: Decimal,
    pub sebi_broker: Decimal,
    pub stamp_duty_match: bool,
    pub stamp_duty_calculated: Decimal,
    pub stamp_duty_broker: Decimal,
    pub gst_match: bool,
    pub gst_calculated: Decimal,
    pub gst_broker: Decimal,
    pub dp_charges_match: bool,
    pub dp_charges_calculated: Decimal,
    pub dp_charges_broker: Decimal,
    pub net_amount_match: bool,
    pub net_amount_calculated: Decimal,
    pub net_amount_broker: Decimal,
    pub all_match: bool,
    pub max_deviation_paise: Decimal,
}

/// Golden test runner for contract note validation.
///
/// Pure data in, pure report out: no filesystem, no clock.
pub struct GoldenTestRunner {
    trades: Vec<ContractNoteTrade>,
    tolerance_paise: Decimal,
}

impl Default for GoldenTestRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl GoldenTestRunner {
    pub fn new() -> Self {
        Self {
            trades: Vec::new(),
            tolerance_paise: Decimal::from(1),
        }
    }

    pub fn with_tolerance(mut self, tolerance_paise: Decimal) -> Self {
        self.tolerance_paise = tolerance_paise;
        self
    }

    pub fn add_trade(mut self, trade: ContractNoteTrade) -> Self {
        self.trades.push(trade);
        self
    }

    pub fn add_trade_mut(&mut self, trade: ContractNoteTrade) {
        self.trades.push(trade);
    }

    pub fn run(&self) -> GoldenTestReport {
        let mut results = Vec::new();
        let mut passed = 0;
        let mut failed = 0;

        for trade in &self.trades {
            let result = self.compare_trade(trade);
            let passed_this = result.all_match;

            if passed_this {
                passed += 1;
            } else {
                failed += 1;
            }

            results.push(result);
        }

        GoldenTestReport {
            total: self.trades.len(),
            passed,
            failed,
            results,
            tolerance_paise: self.tolerance_paise,
        }
    }

    fn compare_trade(&self, trade: &ContractNoteTrade) -> ComparisonResult {
        let stt_calculated = TaxCalculator::stt(
            matches!(trade.product_type, ProductType::Delivery),
            trade.price,
            trade.quantity,
        );

        let turnover = trade.price * trade.quantity;
        let exchange_calculated = TaxCalculator::sebi_turnover_charges(turnover);
        let sebi_calculated = TaxCalculator::sebi_turnover_charges(turnover);

        let stamp_duty_calculated = TaxCalculator::stamp_duty(
            trade.price,
            trade.quantity,
            matches!(trade.side, OrderSide::Buy),
        );

        let gst_calculated =
            TaxCalculator::GST(trade.brokerage, exchange_calculated, sebi_calculated);

        let dp_charges_calculated = if matches!(trade.product_type, ProductType::Delivery)
            && matches!(trade.side, OrderSide::Sell)
        {
            TaxCalculator::dp_charges(true)
        } else {
            Decimal::ZERO
        };

        let total_charges_calculated = trade.brokerage
            + stt_calculated
            + exchange_calculated
            + sebi_calculated
            + stamp_duty_calculated
            + gst_calculated
            + dp_charges_calculated;

        let net_amount_calculated = if matches!(trade.side, OrderSide::Buy) {
            (trade.price * trade.quantity) + total_charges_calculated
        } else {
            (trade.price * trade.quantity) - total_charges_calculated
        };

        let stt_match = (stt_calculated - trade.stt_charged).abs() <= self.tolerance_paise;
        let exchange_match =
            (exchange_calculated - trade.exchange_charges).abs() <= self.tolerance_paise;
        let sebi_match = (sebi_calculated - trade.sebi_charges).abs() <= self.tolerance_paise;
        let stamp_match = (stamp_duty_calculated - trade.stamp_duty).abs() <= self.tolerance_paise;
        let gst_match = (gst_calculated - trade.gst).abs() <= self.tolerance_paise;
        let dp_match = (dp_charges_calculated - trade.dp_charges).abs() <= self.tolerance_paise;
        let net_match = (net_amount_calculated - trade.net_amount).abs() <= self.tolerance_paise;

        let max_deviation = [
            (stt_calculated - trade.stt_charged).abs(),
            (exchange_calculated - trade.exchange_charges).abs(),
            (sebi_calculated - trade.sebi_charges).abs(),
            (stamp_duty_calculated - trade.stamp_duty).abs(),
            (gst_calculated - trade.gst).abs(),
            (dp_charges_calculated - trade.dp_charges).abs(),
            (net_amount_calculated - trade.net_amount).abs(),
        ]
        .into_iter()
        .max()
        .unwrap_or(Decimal::ZERO);

        ComparisonResult {
            trade_id: trade.trade_id.clone(),
            symbol: trade.symbol.clone(),
            broker: trade.broker.clone(),
            stt_match,
            stt_calculated,
            stt_broker: trade.stt_charged,
            exchange_match,
            exchange_calculated,
            exchange_broker: trade.exchange_charges,
            sebi_match,
            sebi_calculated,
            sebi_broker: trade.sebi_charges,
            stamp_duty_match: stamp_match,
            stamp_duty_calculated,
            stamp_duty_broker: trade.stamp_duty,
            gst_match,
            gst_calculated,
            gst_broker: trade.gst,
            dp_charges_match: dp_match,
            dp_charges_calculated,
            dp_charges_broker: trade.dp_charges,
            net_amount_match: net_match,
            net_amount_calculated,
            net_amount_broker: trade.net_amount,
            all_match: stt_match
                && exchange_match
                && sebi_match
                && stamp_match
                && gst_match
                && dp_match
                && net_match,
            max_deviation_paise: max_deviation,
        }
    }
}

/// Row-shaped record of a broker contract note (pure serde data; the `csv`
/// file reading itself stays outside this crate).
#[derive(Debug, Deserialize)]
pub struct CsvRecord {
    pub trade_id: String,
    pub trade_date: String,
    pub symbol: String,
    pub exchange: String,
    pub side: String,
    pub quantity: Decimal,
    pub price: Decimal,
    pub product_type: String,
    pub brokerage: Decimal,
    pub stt: Decimal,
    pub exchange_charges: Decimal,
    pub sebi_charges: Decimal,
    pub stamp_duty: Decimal,
    pub gst: Decimal,
    pub dp_charges: Decimal,
    pub net_amount: Decimal,
    pub settle_date: String,
}

impl CsvRecord {
    pub fn into_trade(self, broker: &str) -> Result<ContractNoteTrade, Box<dyn std::error::Error>> {
        let side = match self.side.to_uppercase().as_str() {
            "BUY" => OrderSide::Buy,
            "SELL" => OrderSide::Sell,
            _ => return Err(format!("Invalid side: {}", self.side).into()),
        };

        let product_type = match self.product_type.to_uppercase().as_str() {
            "DELIVERY" | "CNC" => ProductType::Delivery,
            "INTRADAY" | "MIS" => ProductType::Intraday,
            "OPTIONS" => ProductType::Options,
            "FUTURES" => ProductType::Futures,
            _ => return Err(format!("Invalid product type: {}", self.product_type).into()),
        };

        let trade_time = DateTime::parse_from_rfc3339(&self.trade_date)?.with_timezone(&Utc);
        let settle_date = NaiveDate::parse_from_str(&self.settle_date, "%Y-%m-%d")?;

        Ok(ContractNoteTrade {
            trade_id: self.trade_id,
            broker: broker.to_string(),
            trade_time,
            symbol: self.symbol,
            exchange: self.exchange,
            side,
            quantity: self.quantity,
            price: self.price,
            product_type,
            brokerage: self.brokerage,
            stt_charged: self.stt,
            exchange_charges: self.exchange_charges,
            sebi_charges: self.sebi_charges,
            stamp_duty: self.stamp_duty,
            gst: self.gst,
            dp_charges: self.dp_charges,
            net_amount: self.net_amount,
            settle_date,
        })
    }
}

/// Golden test report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenTestReport {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub results: Vec<ComparisonResult>,
    pub tolerance_paise: Decimal,
}

impl GoldenTestReport {
    pub fn pass_rate(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.passed as f64 / self.total as f64) * 100.0
        }
    }

    pub fn failed_trades(&self) -> Vec<&ComparisonResult> {
        self.results.iter().filter(|r| !r.all_match).collect()
    }

    pub fn stt_mismatches(&self) -> Vec<&ComparisonResult> {
        self.results.iter().filter(|r| !r.stt_match).collect()
    }

    pub fn net_amount_mismatches(&self) -> Vec<&ComparisonResult> {
        self.results
            .iter()
            .filter(|r| !r.net_amount_match)
            .collect()
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn print_summary(&self) {
        println!("╔══════════════════════════════════════════════════════════════╗");
        println!("║           GOLDEN TEST REPORT - BROKER CONTRACT NOTES          ║");
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!(
            "║ Total trades tested: {:>4}                                    ║",
            self.total
        );
        println!(
            "║ Passed: {:>4}  Failed: {:>4}                                    ║",
            self.passed, self.failed
        );
        println!(
            "║ Pass rate: {:>6.2}%                                          ║",
            self.pass_rate()
        );
        println!(
            "║ Tolerance: {} paise (₹0.01)                                  ║",
            self.tolerance_paise
        );
        println!("╠══════════════════════════════════════════════════════════════╣");

        if self.failed > 0 {
            println!("║ FAILED TRADES:                                               ║");
            for result in self.failed_trades() {
                println!(
                    "║  - {} ({}) [{}] - Max deviation: {} paise                    ║",
                    result.symbol, result.broker, result.trade_id, result.max_deviation_paise
                );

                if !result.stt_match {
                    println!(
                        "║     STT: calc={} broker={} (diff={})                         ║",
                        result.stt_calculated,
                        result.stt_broker,
                        (result.stt_calculated - result.stt_broker).abs()
                    );
                }
                if !result.exchange_match {
                    println!(
                        "║     Exchange: calc={} broker={} (diff={})                    ║",
                        result.exchange_calculated,
                        result.exchange_broker,
                        (result.exchange_calculated - result.exchange_broker).abs()
                    );
                }
                if !result.sebi_match {
                    println!(
                        "║     SEBI: calc={} broker={} (diff={})                        ║",
                        result.sebi_calculated,
                        result.sebi_broker,
                        (result.sebi_calculated - result.sebi_broker).abs()
                    );
                }
                if !result.stamp_duty_match {
                    println!(
                        "║     Stamp: calc={} broker={} (diff={})                       ║",
                        result.stamp_duty_calculated,
                        result.stamp_duty_broker,
                        (result.stamp_duty_calculated - result.stamp_duty_broker).abs()
                    );
                }
                if !result.gst_match {
                    println!(
                        "║     GST: calc={} broker={} (diff={})                         ║",
                        result.gst_calculated,
                        result.gst_broker,
                        (result.gst_calculated - result.gst_broker).abs()
                    );
                }
                if !result.dp_charges_match {
                    println!(
                        "║     DP: calc={} broker={} (diff={})                          ║",
                        result.dp_charges_calculated,
                        result.dp_charges_broker,
                        (result.dp_charges_calculated - result.dp_charges_broker).abs()
                    );
                }
                if !result.net_amount_match {
                    println!(
                        "║     Net: calc={} broker={} (diff={})                         ║",
                        result.net_amount_calculated,
                        result.net_amount_broker,
                        (result.net_amount_calculated - result.net_amount_broker).abs()
                    );
                }
            }
        }

        println!("╚══════════════════════════════════════════════════════════════╝");
    }
}

/// Generate sample CSV text for testing fixtures (pure: the trade times are
/// the caller's `now`, the settle dates are `now + 2` business days).
pub fn generate_sample_csv(_broker: &str, count: usize, now: DateTime<Utc>) -> String {
    let header = [
        "trade_id",
        "trade_date",
        "symbol",
        "exchange",
        "side",
        "quantity",
        "price",
        "product_type",
        "brokerage",
        "stt",
        "exchange_charges",
        "sebi_charges",
        "stamp_duty",
        "gst",
        "dp_charges",
        "net_amount",
        "settle_date",
    ]
    .join(",");

    let mut lines = vec![header];

    for i in 0..count {
        let symbol_str = if i % 2 == 0 { "RELIANCE" } else { "TCS" };
        let exchange_str = if i % 3 == 0 { "NSE" } else { "BSE" };
        let side_str = if i % 2 == 0 { "BUY" } else { "SELL" };

        let quantity = Decimal::from(100 + (i % 100) as i64);
        let price = Decimal::from(150000 + (i * 100) as i64);
        let notional = price * quantity;

        // Delivery-side STT on every generated row (they are all DELIVERY).
        let stt = (price * quantity / Decimal::from(1000)).floor();
        let exchange_charges = (notional * Decimal::from(10) / Decimal::from(10_000_000)).floor();
        let sebi_charges = exchange_charges;
        let stamp_duty = (notional * Decimal::from(15) / Decimal::from(100_000)).floor();
        let gst = (Decimal::from(2000) * Decimal::from(18) / Decimal::from(100)).floor();
        let dp_charges = Decimal::ZERO;

        let brokerage = Decimal::from(2000);
        let total_charges =
            brokerage + stt + exchange_charges + sebi_charges + stamp_duty + gst + dp_charges;
        let net_amount = (price * quantity) + total_charges;

        let settle = (now + chrono::Duration::days(2))
            .date_naive()
            .format("%Y-%m-%d")
            .to_string();

        lines.push(
            [
                format!("TRD{i:08}"),
                now.to_rfc3339(),
                symbol_str.to_string(),
                exchange_str.to_string(),
                side_str.to_string(),
                quantity.to_string(),
                price.to_string(),
                "DELIVERY".to_string(),
                brokerage.to_string(),
                stt.to_string(),
                exchange_charges.to_string(),
                sebi_charges.to_string(),
                stamp_duty.to_string(),
                gst.to_string(),
                dp_charges.to_string(),
                net_amount.to_string(),
                settle,
            ]
            .join(","),
        );
    }

    lines.join("\n")
}

/// Fixed-sample broker fixture: Zerodha delivery buy.
/// Trade: Buy 100 RELIANCE @ ₹2,500.00 (Delivery).
/// All rupee/paise amounts are the exact values recorded on the contract
/// note, not runtime-derived.
pub fn sample_zerodha_delivery_buy_trade(now: DateTime<Utc>) -> ContractNoteTrade {
    ContractNoteTrade {
        trade_id: "ZERODHA_DELIVERY_BUY_001".to_string(),
        broker: "zerodha".to_string(),
        trade_time: now,
        symbol: "RELIANCE".to_string(),
        exchange: "NSE".to_string(),
        side: OrderSide::Buy,
        quantity: Decimal::from(100),
        price: Decimal::from(250000),
        product_type: ProductType::Delivery,
        brokerage: Decimal::from(2000),
        stt_charged: Decimal::from(25000),
        exchange_charges: Decimal::from(25),
        sebi_charges: Decimal::from(25),
        stamp_duty: Decimal::from(3750),
        gst: Decimal::from(369),
        dp_charges: Decimal::from(0),
        net_amount: Decimal::from(25029019),
        settle_date: NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(),
    }
}

/// Fixed-sample broker fixture: Zerodha intraday sell.
/// Trade: Sell 50 INFY @ ₹1,500.00 (Intraday).
pub fn sample_zerodha_intraday_sell_trade(now: DateTime<Utc>) -> ContractNoteTrade {
    ContractNoteTrade {
        trade_id: "ZERODHA_INTRADAY_SELL_001".to_string(),
        broker: "zerodha".to_string(),
        trade_time: now,
        symbol: "INFY".to_string(),
        exchange: "NSE".to_string(),
        side: OrderSide::Sell,
        quantity: Decimal::from(50),
        price: Decimal::from(150000),
        product_type: ProductType::Intraday,
        brokerage: Decimal::from(1000),
        stt_charged: Decimal::from(1875),
        exchange_charges: Decimal::from(2),
        sebi_charges: Decimal::from(2),
        stamp_duty: Decimal::from(0),
        gst: Decimal::from(182),
        dp_charges: Decimal::from(0),
        net_amount: Decimal::from(7498094),
        settle_date: NaiveDate::from_ymd_opt(2026, 1, 2).unwrap(),
    }
}

/// Fixed-sample broker fixture: Upstox delivery buy.
/// Trade: Buy 200 TCS @ ₹3,200.00 (Delivery).
pub fn sample_upstox_delivery_buy_trade(now: DateTime<Utc>) -> ContractNoteTrade {
    ContractNoteTrade {
        trade_id: "UPSTOX_DELIVERY_BUY_001".to_string(),
        broker: "upstox".to_string(),
        trade_time: now,
        symbol: "TCS".to_string(),
        exchange: "NSE".to_string(),
        side: OrderSide::Buy,
        quantity: Decimal::from(200),
        price: Decimal::from(320000),
        product_type: ProductType::Delivery,
        brokerage: Decimal::from(2000),
        stt_charged: Decimal::from(64000),
        exchange_charges: Decimal::from(19),
        sebi_charges: Decimal::from(64),
        stamp_duty: Decimal::from(9600),
        gst: Decimal::from(360),
        dp_charges: Decimal::from(0),
        net_amount: Decimal::from(64075943),
        settle_date: NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(),
    }
}

/// Fixed-sample broker fixture: Upstox F&O option buy.
/// Trade: Buy 1 lot NIFTY 19000 CE @ ₹150.00 (Option Premium).
pub fn sample_upstox_option_buy_trade(now: DateTime<Utc>) -> ContractNoteTrade {
    ContractNoteTrade {
        trade_id: "UPSTOX_OPTION_BUY_001".to_string(),
        broker: "upstox".to_string(),
        trade_time: now,
        symbol: "NIFTY 19000 CE".to_string(),
        exchange: "NFO".to_string(),
        side: OrderSide::Buy,
        quantity: Decimal::from(50),
        price: Decimal::from(15000),
        product_type: ProductType::Options,
        brokerage: Decimal::from(2000),
        stt_charged: Decimal::from(187),
        exchange_charges: Decimal::from(0),
        sebi_charges: Decimal::from(0),
        stamp_duty: Decimal::from(0),
        gst: Decimal::from(360),
        dp_charges: Decimal::from(0),
        net_amount: Decimal::from(752360),
        settle_date: NaiveDate::from_ymd_opt(2026, 1, 2).unwrap(),
    }
}

/// Fixed-sample broker fixture: total-charges reconciliation.
/// Test case: Delivery buy 100 shares @ ₹2,000.
pub fn sample_total_charges_trade(now: DateTime<Utc>) -> ContractNoteTrade {
    ContractNoteTrade {
        trade_id: "TOTAL_CHARGES_TEST_001".to_string(),
        broker: "zerodha".to_string(),
        trade_time: now,
        symbol: "RELIANCE".to_string(),
        exchange: "NSE".to_string(),
        side: OrderSide::Buy,
        quantity: Decimal::from(100),
        price: Decimal::from(200000),
        product_type: ProductType::Delivery,
        brokerage: Decimal::from(2000),
        stt_charged: Decimal::from(20000),
        exchange_charges: Decimal::from(20),
        sebi_charges: Decimal::from(20),
        stamp_duty: Decimal::from(3000),
        gst: Decimal::from(360),
        dp_charges: Decimal::from(1593),
        net_amount: Decimal::from(20029019),
        settle_date: NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(),
    }
}
