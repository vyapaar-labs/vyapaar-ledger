//! # व्यापार・खातावही vyapaar-ledger-journal
//!
//! Double-entry general ledger for the Indian equity trading lifecycle and the
//! Indian statutory tax calculator (STT, GST, Stamp Duty, SEBI fees, DP
//! charges, exchange turnover fee).
//!
//! Pure zero-I/O: no sockets, no filesystem, no database drivers. Temporal
//! determinism: every entry receives an explicit `DateTime<Utc>` from the
//! caller; the library never mints a timestamp.
//!
//! # Migration notes (Tier 1B, `honba-kokoro-ledger` → `vyapaar-ledger-journal`)
//!
//! - The chart of accounts is now keyed by the strongly typed
//!   [`AccountCode`] (migrated out of the old facade repository layer into
//!   this, the ledger crate, where it belongs). The duplicated legacy account
//!   names (`CashAtBroker`, `IncomeRealizedPnL`, …) and the legacy
//!   `Account` enum were deleted; the aggregate helpers read exactly one
//!   canonical code each.
//! - [`Transaction`] and [`DoubleEntryLedger`] maps are keyed by
//!   [`AccountCode`], not `String`.
//! - The local `Exchange { Nse, Bse }` was deleted in favour of the canonical
//!   [`vyapaar_core::Exchange`]; `compute_charges` maps `Bse` to the BSE
//!   turnover rate and every other segment (NSE, NFO, MCX, CDS) to the NSE
//!   cash-segment rate until per-segment fee tables arrive with real
//!   exchange data.
//! - `TradeSide` is the canonical [`vyapaar_core::OrderSide`] (serde
//!   UPPERCASE) instead of a duplicate enum.
//! - The broker contract-note golden framework now lives in
//!   [`contract_notes`] as a pure library module: the `load_from_csv` /
//!   `load_from_json` filesystem loaders were dropped (zero-I/O gate), sample
//!   fixtures take an explicit `now: DateTime<Utc>` instead of calling
//!   `Utc::now()`, and `generate_sample_csv` writes CSV text directly.
//! - The legacy floor-rounded statutory functions are retained verbatim for
//!   the paise-denominated contract-note fixtures; the newer canonical
//!   methods round half-even to 2 dp.

mod contract_notes;

pub use contract_notes::{
    ComparisonResult, ContractNoteTrade, CsvRecord, GoldenTestReport, GoldenTestRunner,
    ProductType, generate_sample_csv, sample_total_charges_trade, sample_upstox_delivery_buy_trade,
    sample_upstox_option_buy_trade, sample_zerodha_delivery_buy_trade,
    sample_zerodha_intraday_sell_trade,
};

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use std::collections::BTreeMap;
use thiserror::Error;
use vyapaar_core::OrderSide;

/// Error types for ledger operations.
#[derive(Debug, Error)]
pub enum LedgerError {
    #[error("Invalid entry amount")]
    InvalidAmount,
    #[error("Duplicate entry ID: {0}")]
    DuplicateEntry(String),
    #[error("Insufficient balance")]
    InsufficientBalance,
    #[error("Unknown account: {0}")]
    UnknownAccount(String),
}

/// Canonical ledger account code in the Chart of Accounts.
///
/// Migrated from the old facade repository layer: the shared-kernel
/// vocabulary this crate owns is the chart of accounts itself. The
/// associated constants are the single spelling of each account code; the
/// duplicated legacy short names (`CashAtBroker`, `IncomeRealizedPnL`, …)
/// were deleted rather than aliased.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct AccountCode(pub String);

impl AccountCode {
    pub const CASH_AT_BROKER: &'static str = "Assets:Cash:BrokerAccount";
    pub const HOLDINGS_EQUITY_DELIVERY: &'static str = "Assets:Holdings:EquityDelivery";
    pub const LIABILITIES_MARGIN_PAYABLE: &'static str = "Liabilities:MarginPayable";
    pub const OPENING_CAPITAL: &'static str = "Equity:OpeningCapital";
    pub const EXPENSES_BROKERAGE: &'static str = "Expenses:Brokerage";
    pub const EXPENSES_TAXES_STT: &'static str = "Expenses:Taxes:STT";
    pub const EXPENSES_TAXES_GST: &'static str = "Expenses:Taxes:GST";
    pub const EXPENSES_TAXES_STAMP_DUTY: &'static str = "Expenses:Taxes:StampDuty";
    pub const EXPENSES_EXCHANGE_FEES: &'static str = "Expenses:ExchangeFees";
    pub const EXPENSES_SEBI_FEES: &'static str = "Expenses:SEBIFees";
    pub const EXPENSES_DP_CHARGES: &'static str = "Expenses:DPCharges";
    pub const INCOME_REALIZED_PNL: &'static str = "Income:RealizedPnL";

    /// Wrap a raw chart-of-accounts code.
    #[must_use]
    pub fn new(code: impl Into<String>) -> Self {
        Self(code.into())
    }

    /// Borrow the raw code.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for AccountCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for AccountCode {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for AccountCode {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// A balanced double-entry journal transaction.
/// Every transaction has equal total debits and credits across the chart of accounts.
#[derive(Debug, Clone, PartialEq)]
pub struct Transaction {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub debits: BTreeMap<AccountCode, Decimal>,
    pub credits: BTreeMap<AccountCode, Decimal>,
    pub description: String,
}

/// Post a transaction to the ledger.
/// Ensures debits total equals credits total (double-entry bookkeeping).
pub fn post_transaction(transaction: &Transaction) -> Result<(), LedgerError> {
    let total_debits: Decimal = transaction.debits.values().sum();
    let total_credits: Decimal = transaction.credits.values().sum();

    if total_debits != total_credits {
        return Err(LedgerError::InvalidAmount);
    }

    Ok(())
}

/// Canonical Double-Entry General Ledger.
/// Enforces debit-credit equilibrium for every transaction and maintains
/// balance sheet integrity across the full Indian equity trading lifecycle.
#[derive(Debug, Clone, Default)]
pub struct DoubleEntryLedger {
    pub debit_balances: BTreeMap<AccountCode, Decimal>,
    pub credit_balances: BTreeMap<AccountCode, Decimal>,
    pub transactions: Vec<Transaction>,
}

impl DoubleEntryLedger {
    /// Create a new empty double-entry ledger.
    pub fn new() -> Self {
        Self::default()
    }

    /// Post a balanced transaction to the ledger.
    /// Returns `Ok(())` if debits match credits, or `Err(LedgerError::InvalidAmount)` if unbalanced.
    pub fn post(&mut self, transaction: Transaction) -> Result<(), LedgerError> {
        let total_debits: Decimal = transaction.debits.values().copied().sum();
        let total_credits: Decimal = transaction.credits.values().copied().sum();

        if total_debits != total_credits {
            return Err(LedgerError::InvalidAmount);
        }

        // Validate non-negative amounts (each leg must be > 0)
        for amount in transaction.debits.values() {
            if *amount <= Decimal::ZERO {
                return Err(LedgerError::InvalidAmount);
            }
        }
        for amount in transaction.credits.values() {
            if *amount <= Decimal::ZERO {
                return Err(LedgerError::InvalidAmount);
            }
        }

        for (account, amount) in &transaction.debits {
            *self.debit_balances.entry(account.clone()).or_default() += *amount;
        }
        for (account, amount) in &transaction.credits {
            *self.credit_balances.entry(account.clone()).or_default() += *amount;
        }

        self.transactions.push(transaction);
        Ok(())
    }

    /// Total sum of all debits posted across all accounts.
    pub fn total_debits(&self) -> Decimal {
        self.debit_balances.values().copied().sum()
    }

    /// Total sum of all credits posted across all accounts.
    pub fn total_credits(&self) -> Decimal {
        self.credit_balances.values().copied().sum()
    }

    /// Net debit balance for an account (Debits - Credits).
    pub fn net_debit_balance(&self, account: &AccountCode) -> Decimal {
        let debits = self
            .debit_balances
            .get(account)
            .copied()
            .unwrap_or(Decimal::ZERO);
        let credits = self
            .credit_balances
            .get(account)
            .copied()
            .unwrap_or(Decimal::ZERO);
        debits - credits
    }

    /// Net credit balance for an account (Credits - Debits).
    pub fn net_credit_balance(&self, account: &AccountCode) -> Decimal {
        let debits = self
            .debit_balances
            .get(account)
            .copied()
            .unwrap_or(Decimal::ZERO);
        let credits = self
            .credit_balances
            .get(account)
            .copied()
            .unwrap_or(Decimal::ZERO);
        credits - debits
    }

    /// Current cash balance at broker.
    pub fn cash_balance(&self) -> Decimal {
        self.net_debit_balance(&AccountCode::new(AccountCode::CASH_AT_BROKER))
    }

    /// Current holdings at acquisition cost.
    pub fn holdings_balance(&self) -> Decimal {
        self.net_debit_balance(&AccountCode::new(AccountCode::HOLDINGS_EQUITY_DELIVERY))
    }

    /// Opening equity / capital deposited.
    pub fn opening_capital(&self) -> Decimal {
        self.net_credit_balance(&AccountCode::new(AccountCode::OPENING_CAPITAL))
    }

    /// Realized PnL from trades.
    pub fn realized_pnl(&self) -> Decimal {
        self.net_credit_balance(&AccountCode::new(AccountCode::INCOME_REALIZED_PNL))
    }

    /// Brokerage incurred.
    pub fn brokerage(&self) -> Decimal {
        self.net_debit_balance(&AccountCode::new(AccountCode::EXPENSES_BROKERAGE))
    }

    /// Statutory charges incurred (STT + Stamp Duty + Exchange Fees + SEBI Fees + GST + DP Charges).
    pub fn statutory_charges(&self) -> Decimal {
        [
            AccountCode::EXPENSES_TAXES_STT,
            AccountCode::EXPENSES_TAXES_STAMP_DUTY,
            AccountCode::EXPENSES_EXCHANGE_FEES,
            AccountCode::EXPENSES_SEBI_FEES,
            AccountCode::EXPENSES_TAXES_GST,
            AccountCode::EXPENSES_DP_CHARGES,
        ]
        .into_iter()
        .map(AccountCode::new)
        .map(|code| self.net_debit_balance(&code))
        .sum()
    }

    /// Verifies the fundamental balance sheet accounting identity:
    /// Cash + Holdings == Opening Capital + Realized PnL - Statutory Charges - Brokerage
    pub fn verify_accounting_identity(&self) -> Result<(), (Decimal, Decimal)> {
        let lhs = self.cash_balance() + self.holdings_balance();
        let rhs = self.opening_capital() + self.realized_pnl()
            - self.statutory_charges()
            - self.brokerage();
        if lhs == rhs { Ok(()) } else { Err((lhs, rhs)) }
    }

    /// Verifies the universal double-entry identity:
    /// sum(Debits) == sum(Credits)
    pub fn verify_double_entry_balance(&self) -> bool {
        self.total_debits() == self.total_credits()
    }

    /// Helper to record opening capital deposit.
    pub fn record_opening_capital(
        &mut self,
        tx_id: &str,
        timestamp: DateTime<Utc>,
        amount: Decimal,
    ) -> Result<(), LedgerError> {
        let mut debits = BTreeMap::new();
        debits.insert(AccountCode::new(AccountCode::CASH_AT_BROKER), amount);
        let mut credits = BTreeMap::new();
        credits.insert(AccountCode::new(AccountCode::OPENING_CAPITAL), amount);

        self.post(Transaction {
            id: tx_id.to_string(),
            timestamp,
            debits,
            credits,
            description: "Opening capital deposit".to_string(),
        })
    }

    /// Helper to record an equity buy execution with statutory charges.
    pub fn record_buy(
        &mut self,
        tx_id: &str,
        timestamp: DateTime<Utc>,
        notional: Decimal,
        charges: &TaxBreakdown,
    ) -> Result<(), LedgerError> {
        let total_charges = charges.total();
        let total_cash = notional + total_charges;

        let mut debits = BTreeMap::new();
        debits.insert(
            AccountCode::new(AccountCode::HOLDINGS_EQUITY_DELIVERY),
            notional,
        );
        if charges.brokerage > Decimal::ZERO {
            debits.insert(
                AccountCode::new(AccountCode::EXPENSES_BROKERAGE),
                charges.brokerage,
            );
        }
        if charges.stt > Decimal::ZERO {
            debits.insert(
                AccountCode::new(AccountCode::EXPENSES_TAXES_STT),
                charges.stt,
            );
        }
        if charges.stamp_duty > Decimal::ZERO {
            debits.insert(
                AccountCode::new(AccountCode::EXPENSES_TAXES_STAMP_DUTY),
                charges.stamp_duty,
            );
        }
        if charges.exchange_turnover > Decimal::ZERO {
            debits.insert(
                AccountCode::new(AccountCode::EXPENSES_EXCHANGE_FEES),
                charges.exchange_turnover,
            );
        }
        if charges.sebi_turnover > Decimal::ZERO {
            debits.insert(
                AccountCode::new(AccountCode::EXPENSES_SEBI_FEES),
                charges.sebi_turnover,
            );
        }
        if charges.gst > Decimal::ZERO {
            debits.insert(
                AccountCode::new(AccountCode::EXPENSES_TAXES_GST),
                charges.gst,
            );
        }
        if charges.dp_charge > Decimal::ZERO {
            debits.insert(
                AccountCode::new(AccountCode::EXPENSES_DP_CHARGES),
                charges.dp_charge,
            );
        }

        let mut credits = BTreeMap::new();
        credits.insert(AccountCode::new(AccountCode::CASH_AT_BROKER), total_cash);

        self.post(Transaction {
            id: tx_id.to_string(),
            timestamp,
            debits,
            credits,
            description: "Equity buy execution".to_string(),
        })
    }

    /// Helper to record an equity sell execution with cost-basis relief, realized PnL, and statutory charges.
    pub fn record_sell(
        &mut self,
        tx_id: &str,
        timestamp: DateTime<Utc>,
        proceeds: Decimal,
        cost_basis: Decimal,
        charges: &TaxBreakdown,
    ) -> Result<(), LedgerError> {
        let total_charges = charges.total();
        let pnl = proceeds - cost_basis;

        let mut debits = BTreeMap::new();
        let mut credits = BTreeMap::new();

        if proceeds >= total_charges {
            let cash_received = proceeds - total_charges;
            if cash_received > Decimal::ZERO {
                debits.insert(AccountCode::new(AccountCode::CASH_AT_BROKER), cash_received);
            }
        } else {
            let cash_deficit = total_charges - proceeds;
            credits.insert(AccountCode::new(AccountCode::CASH_AT_BROKER), cash_deficit);
        }
        if charges.brokerage > Decimal::ZERO {
            debits.insert(
                AccountCode::new(AccountCode::EXPENSES_BROKERAGE),
                charges.brokerage,
            );
        }
        if charges.stt > Decimal::ZERO {
            debits.insert(
                AccountCode::new(AccountCode::EXPENSES_TAXES_STT),
                charges.stt,
            );
        }
        if charges.stamp_duty > Decimal::ZERO {
            debits.insert(
                AccountCode::new(AccountCode::EXPENSES_TAXES_STAMP_DUTY),
                charges.stamp_duty,
            );
        }
        if charges.exchange_turnover > Decimal::ZERO {
            debits.insert(
                AccountCode::new(AccountCode::EXPENSES_EXCHANGE_FEES),
                charges.exchange_turnover,
            );
        }
        if charges.sebi_turnover > Decimal::ZERO {
            debits.insert(
                AccountCode::new(AccountCode::EXPENSES_SEBI_FEES),
                charges.sebi_turnover,
            );
        }
        if charges.gst > Decimal::ZERO {
            debits.insert(
                AccountCode::new(AccountCode::EXPENSES_TAXES_GST),
                charges.gst,
            );
        }
        if charges.dp_charge > Decimal::ZERO {
            debits.insert(
                AccountCode::new(AccountCode::EXPENSES_DP_CHARGES),
                charges.dp_charge,
            );
        }

        credits.insert(
            AccountCode::new(AccountCode::HOLDINGS_EQUITY_DELIVERY),
            cost_basis,
        );

        if pnl >= Decimal::ZERO {
            if pnl > Decimal::ZERO {
                credits.insert(AccountCode::new(AccountCode::INCOME_REALIZED_PNL), pnl);
            }
        } else {
            let loss = -pnl;
            debits.insert(AccountCode::new(AccountCode::INCOME_REALIZED_PNL), loss);
        }

        self.post(Transaction {
            id: tx_id.to_string(),
            timestamp,
            debits,
            credits,
            description: "Equity sell execution".to_string(),
        })
    }
}

/// Indian statutory tax calculator.
/// Computes STT, GST, Stamp Duty, SEBI fees, and DP charges per SEBI
/// and Finance Act rules for equity, F&O, and commodity derivatives.
pub struct TaxCalculator;

impl TaxCalculator {
    pub fn stt(delivery: bool, price: Decimal, quantity: Decimal) -> Decimal {
        let rate = if delivery {
            Decimal::from(1i64) / Decimal::from(1000i64) // 0.1%
        } else {
            Decimal::from(1i64) / Decimal::from(4000i64) // 0.025%
        };

        let notional = price * quantity;
        (notional * rate).floor()
    }

    #[allow(non_snake_case)]
    pub fn GST(brokerage: Decimal, exchange_charges: Decimal, sebi_fees: Decimal) -> Decimal {
        let total = brokerage + exchange_charges + sebi_fees;
        (total * Decimal::from(18i64) / Decimal::from(100i64)).floor()
    }

    pub fn stamp_duty(price: Decimal, quantity: Decimal, buy: bool) -> Decimal {
        let rate = if buy {
            Decimal::from(15i64) / Decimal::from(100000i64) // 0.015%
        } else {
            Decimal::from(0i64)
        };
        (price * quantity * rate).floor()
    }

    pub fn sebi_turnover_charges(turnover: Decimal) -> Decimal {
        (turnover * Decimal::from(10i64) / Decimal::from(10_000_000i32)).floor()
    }

    pub fn dp_charges(is_delivery_sale: bool) -> Decimal {
        let base = Decimal::from(1350i64) / Decimal::from(100i64); // ₹13.50
        let gst_rate = Decimal::from(18i64) / Decimal::from(100i64);

        if is_delivery_sale {
            let gst = (base * gst_rate).floor();
            base + gst
        } else {
            Decimal::from(0i64)
        }
    }
}

/// Canonical itemized statutory fee breakdown — the result DTO for
/// [`TaxCalculator::compute_charges`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TaxBreakdown {
    pub brokerage: Decimal,
    pub stt: Decimal,
    pub stamp_duty: Decimal,
    pub exchange_turnover: Decimal,
    pub sebi_turnover: Decimal,
    pub gst: Decimal,
    pub dp_charge: Decimal,
}

impl TaxBreakdown {
    /// Total aggregate transaction cost in INR.
    pub fn total(&self) -> Decimal {
        self.brokerage
            + self.stt
            + self.stamp_duty
            + self.exchange_turnover
            + self.sebi_turnover
            + self.gst
            + self.dp_charge
    }

    /// Accumulate another breakdown into self.
    pub fn add_assign(&mut self, other: &TaxBreakdown) {
        self.brokerage += other.brokerage;
        self.stt += other.stt;
        self.stamp_duty += other.stamp_duty;
        self.exchange_turnover += other.exchange_turnover;
        self.sebi_turnover += other.sebi_turnover;
        self.gst += other.gst;
        self.dp_charge += other.dp_charge;
    }
}

/// Canonical rounding rule for every canonical `TaxCalculator` method below:
/// round-to-nearest-2dp (banker's / round-half-to-even via
/// `Decimal::round_dp`). Deliberate departure from the legacy floor-based
/// methods above, which are retained verbatim for the paise-denominated
/// contract-note fixtures.
fn round_inr(amount: Decimal) -> Decimal {
    amount.round_dp(2)
}

impl TaxCalculator {
    /// Brokerage: percentage of notional, capped at a flat ceiling.
    /// 0.03% of notional, capped at ₹20 per executed order.
    pub fn brokerage(notional: Decimal) -> Decimal {
        if notional <= Decimal::ZERO {
            return Decimal::ZERO;
        }
        let raw = notional * Decimal::new(3, 4); // 0.0003 = 0.03%
        round_inr(raw.min(Decimal::from(20)))
    }

    /// NSE cash-segment exchange turnover charge: 0.00297% of notional.
    pub fn nse_turnover_charges(turnover: Decimal) -> Decimal {
        if turnover <= Decimal::ZERO {
            return Decimal::ZERO;
        }
        round_inr(turnover * Decimal::new(297, 7)) // 0.0000297
    }

    /// BSE cash-segment exchange turnover charge: 0.00375% of notional.
    pub fn bse_turnover_charges(turnover: Decimal) -> Decimal {
        if turnover <= Decimal::ZERO {
            return Decimal::ZERO;
        }
        round_inr(turnover * Decimal::new(375, 7)) // 0.0000375
    }

    /// SEBI regulatory turnover fee: ₹10 per crore of turnover.
    pub fn sebi_fee(turnover: Decimal) -> Decimal {
        if turnover <= Decimal::ZERO {
            return Decimal::ZERO;
        }
        round_inr(turnover * Decimal::new(1, 6)) // 0.000001
    }

    /// Securities Transaction Tax, correctly gated by side AND product type.
    ///
    /// Fixes the real bug in the legacy `stt(delivery: bool, ...)`: that
    /// method has no side parameter and charges the intraday rate
    /// unconditionally whenever `delivery == false`, including on BUY
    /// orders. Real STT only ever charges Intraday/Futures/Options STT on
    /// the SELL leg; Delivery STT is charged on BOTH legs.
    pub fn stt_charge(side: OrderSide, product: ProductType, notional: Decimal) -> Decimal {
        if notional <= Decimal::ZERO {
            return Decimal::ZERO;
        }
        let is_sell = side == OrderSide::Sell;
        let rate = match product {
            ProductType::Delivery => Decimal::new(1, 3), // 0.1%, both sides
            ProductType::Intraday if is_sell => Decimal::new(25, 5), // 0.025%
            ProductType::Futures if is_sell => Decimal::new(125, 6), // 0.0125%
            ProductType::Options if is_sell => Decimal::new(625, 6), // 0.0625%
            _ => return Decimal::ZERO,
        };
        round_inr(notional * rate)
    }

    /// Stamp duty, gated by product type. Applies on the BUY side only
    /// (per the Indian Stamp Act amendment effective July 2020), with
    /// rates that vary by product.
    pub fn stamp_duty_charge(side: OrderSide, product: ProductType, notional: Decimal) -> Decimal {
        if notional <= Decimal::ZERO || side == OrderSide::Sell {
            return Decimal::ZERO;
        }
        let rate = match product {
            ProductType::Delivery => Decimal::new(15, 5), // 0.015%
            ProductType::Intraday => Decimal::new(3, 5),  // 0.003%
            ProductType::Futures => Decimal::new(2, 5),   // 0.002%
            ProductType::Options => Decimal::new(3, 5),   // 0.003%
        };
        round_inr(notional * rate)
    }

    /// Depository Participant charge: a flat fee on the first Delivery
    /// sell of the day for a given scrip. Zero on buys, zero on
    /// non-Delivery products, zero on subsequent sells of the same scrip
    /// the same day. Unlike the legacy `dp_charges`, this returns the RAW
    /// flat fee (no GST pre-folded in) — GST on the DP charge is applied
    /// via `gst_charge`'s taxable base instead.
    pub fn dp_charge(side: OrderSide, product: ProductType, is_first_sell_of_day: bool) -> Decimal {
        if side == OrderSide::Sell && product == ProductType::Delivery && is_first_sell_of_day {
            Decimal::new(1350, 2) // ₹13.50
        } else {
            Decimal::ZERO
        }
    }

    /// GST: 18% on (brokerage + exchange turnover + SEBI turnover + DP charge).
    ///
    /// Explicit judgment call: the legacy `GST(brokerage, exchange_charges,
    /// sebi_fees)` omits `dp_charge` from the taxable base, but the canonical
    /// cost model includes it (`gst_taxable = brokerage + exchange_turnover +
    /// sebi_turnover + dp_charge`). This canonical method follows the cost
    /// model's composition for research-live parity.
    pub fn gst_charge(
        brokerage: Decimal,
        exchange_turnover: Decimal,
        sebi_turnover: Decimal,
        dp_charge: Decimal,
    ) -> Decimal {
        let taxable = brokerage + exchange_turnover + sebi_turnover + dp_charge;
        round_inr(taxable * Decimal::new(18, 2)) // 18%
    }

    /// Single entry point: compute every statutory charge for one fill.
    /// `price` and `quantity` are both plain rupee-denominated `Decimal`
    /// values (consistent with `vyapaar_core::Price`/`Quantity`, not paise);
    /// `notional = price * quantity`.
    ///
    /// Exchange selection maps the canonical [`vyapaar_core::Exchange`] onto
    /// the two cash-segment turnover rates: `Exchange::Bse` uses the BSE
    /// rate; every other segment — NSE plus the derivative segments NFO,
    /// MCX and CDS — falls back to the NSE cash-segment rate until
    /// per-segment fee tables arrive with real exchange data.
    pub fn compute_charges(
        side: OrderSide,
        product: ProductType,
        exchange: vyapaar_core::Exchange,
        price: Decimal,
        quantity: Decimal,
        is_first_sell_of_day: bool,
    ) -> TaxBreakdown {
        if price <= Decimal::ZERO || quantity <= Decimal::ZERO {
            return TaxBreakdown::default();
        }
        let notional = price * quantity;

        let brokerage = Self::brokerage(notional);
        let stt = Self::stt_charge(side, product, notional);
        let stamp_duty = Self::stamp_duty_charge(side, product, notional);
        let exchange_turnover = match exchange {
            vyapaar_core::Exchange::Bse => Self::bse_turnover_charges(notional),
            // NSE, NFO, MCX and CDS all fall back to the NSE cash-segment rate.
            _ => Self::nse_turnover_charges(notional),
        };
        let sebi_turnover = Self::sebi_fee(notional);
        let dp_charge = Self::dp_charge(side, product, is_first_sell_of_day);
        let gst = Self::gst_charge(brokerage, exchange_turnover, sebi_turnover, dp_charge);

        TaxBreakdown {
            brokerage,
            stt,
            stamp_duty,
            exchange_turnover,
            sebi_turnover,
            gst,
            dp_charge,
        }
    }
}
