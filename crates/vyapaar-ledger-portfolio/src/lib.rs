//! # व्यापार・पोर्टफोलियो vyapaar-ledger-portfolio
//!
//! Pure portfolio valuation (ADR 0032, ROADMAP E13). Uses the canonical
//! [`vyapaar_core`] types for Decimal-accurate valuation.
//!
//! No clock and no I/O — the caller stamps `as_of`/`now`/timestamps so the same
//! positions and marks always give a byte-identical snapshot, and the server, a
//! backtest and the browser agree. The engine never places an order: a
//! portfolio is research and presentation, fed by fills and marks (ADR 0032 §7).
//!
//! # Migration notes (Tier 1B, `honba-kokoro-portfolio` → `vyapaar-ledger-portfolio`)
//!
//! - The duplicated local `PositionSide` was deleted; the canonical
//!   `vyapaar_core::PositionSide` is used (and re-exported) instead.
//! - `Fitolot` / `CostBasisLot` were deleted and replaced by the canonical
//!   [`TaxLot`] value object plus the [`MatchedLot`] result and [`LotFill`]
//!   matching-input types; the FIFO matching logic now lives *on the
//!   aggregate* ([`AccountState::match_lots`]) instead of in a repository.
//! - [`HeldPosition`] carries `realized_pnl` and `updated_at` so aggregates can
//!   maintain their own invariants without a repository rewriting them.
//! - [`value`] lost its nonsense parameters (`_as_of: &chrono::Utc`,
//!   `_cash_flow: i64`) and now computes the market value, unrealized and
//!   realized PnL it documents.
//! - The orphaned legacy modules (`engine.rs`, `error.rs`, `marks.rs`,
//!   `state.rs` referencing `honba_entities` / `honba_messages`) were **not**
//!   migrated: they reference dependencies that were never in this crate's
//!   manifest and are superseded by the canonical `vyapaar-core` portfolio wire
//!   types.

use chrono::{DateTime, Datelike, NaiveDate, Utc, Weekday};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// Re-export the canonical kernel vocabulary at the crate root so downstream
/// gates import only this crate.
pub use vyapaar_core::{Currency, InstrumentId, Money, OrderSide, PositionSide, Price, Quantity};

/// Indian clearing calendar for NSE/BSE trading and settlement holidays.
/// Determines valid trading days and settlement days (T+1).
#[derive(Debug, Clone, Default)]
pub struct ClearingCalendar {
    /// Trading holidays (non-business days when markets are closed)
    trading_holidays: HashMap<i32, Vec<NaiveDate>>,
    /// Settlement holidays (when clearing corporation doesn't settle)
    settlement_holidays: HashMap<i32, Vec<NaiveDate>>,
}

impl ClearingCalendar {
    /// Create a new clearing calendar with default NSE holidays.
    pub fn new() -> Self {
        let mut cal = Self {
            trading_holidays: HashMap::new(),
            settlement_holidays: HashMap::new(),
        };
        // Populate with known NSE holidays for 2024-2026
        cal.add_nse_holidays_2024_2026();
        cal
    }

    /// Build a clearing calendar from an injected list of holiday dates,
    /// instead of this crate's own hardcoded NSE table (used by `new()`).
    ///
    /// This lets the real exchange holiday *data* stay owned elsewhere
    /// (e.g. a backtesting `NseTradingCalendar`, which seeds real NSE holidays
    /// and Muhurat trading sessions) without this crate taking a dependency in
    /// that direction or duplicating a potentially-diverging copy of the same
    /// table. Weekend exclusion still applies on top of the injected list.
    /// Settlement holidays are treated identically to trading holidays,
    /// same as `new()`.
    pub fn with_holidays(holidays: &[NaiveDate]) -> Self {
        let mut cal = Self {
            trading_holidays: HashMap::new(),
            settlement_holidays: HashMap::new(),
        };
        for &date in holidays {
            cal.trading_holidays
                .entry(date.year())
                .or_default()
                .push(date);
        }
        cal.settlement_holidays = cal.trading_holidays.clone();
        cal
    }

    /// Add NSE trading and settlement holidays for 2024-2026.
    fn add_nse_holidays_2024_2026(&mut self) {
        // 2024 NSE Holidays
        self.add_trading_holidays(
            2024,
            &[
                "2024-01-26", // Republic Day
                "2024-03-08", // Mahashivratri
                "2024-03-25", // Holi
                "2024-03-29", // Good Friday
                "2024-04-11", // Id-Ul-Fitr (Ramzan Id)
                "2024-04-17", // Ram Navami
                "2024-05-01", // Maharashtra Day
                "2024-06-17", // Bakri Id
                "2024-07-17", // Muharram
                "2024-08-15", // Independence Day
                "2024-10-02", // Mahatma Gandhi Jayanti
                "2024-11-01", // Diwali Laxmi Pujan
                "2024-11-15", // Gurunanak Jayanti
                "2024-12-25", // Christmas
            ],
        );

        // 2025 NSE Holidays
        self.add_trading_holidays(
            2025,
            &[
                "2025-01-26", // Republic Day
                "2025-02-26", // Mahashivratri
                "2025-03-14", // Holi
                "2025-03-31", // Id-Ul-Fitr
                "2025-04-10", // Mahavir Jayanti
                "2025-04-14", // Dr. Baba Saheb Ambedkar Jayanti
                "2025-04-18", // Good Friday
                "2025-05-01", // Maharashtra Day
                "2025-06-07", // Bakri Id
                "2025-07-06", // Muharram
                "2025-08-15", // Independence Day
                "2025-10-02", // Mahatma Gandhi Jayanti
                "2025-10-21", // Diwali Laxmi Pujan
                "2025-11-05", // Gurunanak Jayanti
                "2025-12-25", // Christmas
            ],
        );

        // 2026 NSE Holidays
        self.add_trading_holidays(
            2026,
            &[
                "2026-01-26", // Republic Day
                "2026-02-15", // Mahashivratri
                "2026-03-03", // Holi
                "2026-03-20", // Id-Ul-Fitr
                "2026-04-02", // Mahavir Jayanti
                "2026-04-03", // Good Friday
                "2026-04-14", // Dr. Baba Saheb Ambedkar Jayanti
                "2026-05-01", // Maharashtra Day
                "2026-05-27", // Bakri Id
                "2026-06-26", // Muharram
                "2026-08-15", // Independence Day
                "2026-10-02", // Mahatma Gandhi Jayanti
                "2026-11-08", // Diwali Laxmi Pujan
                "2026-11-24", // Gurunanak Jayanti
                "2026-12-25", // Christmas
            ],
        );

        // Settlement holidays are typically the same as trading holidays
        // but may differ (e.g., Saturday settlements)
        self.settlement_holidays = self.trading_holidays.clone();
    }

    fn add_trading_holidays(&mut self, year: i32, dates: &[&str]) {
        let mut holidays = Vec::new();
        for d in dates {
            if let Ok(date) = NaiveDate::parse_from_str(d, "%Y-%m-%d") {
                holidays.push(date);
            }
        }
        self.trading_holidays.insert(year, holidays);
    }

    /// Check if a date is a trading day (not weekend, not holiday).
    pub fn is_trading_day(&self, date: NaiveDate) -> bool {
        if date.weekday() == Weekday::Sat || date.weekday() == Weekday::Sun {
            return false;
        }
        if let Some(holidays) = self.trading_holidays.get(&date.year())
            && holidays.contains(&date)
        {
            return false;
        }
        true
    }

    /// Check if a date is a settlement day (clearing corporation operational).
    pub fn is_settlement_day(&self, date: NaiveDate) -> bool {
        // Settlement happens on business days (Mon-Fri) excluding settlement holidays
        if date.weekday() == Weekday::Sat || date.weekday() == Weekday::Sun {
            return false;
        }
        if let Some(holidays) = self.settlement_holidays.get(&date.year())
            && holidays.contains(&date)
        {
            return false;
        }
        true
    }

    /// Calculate settlement date for a trade date (T+1 business day).
    /// If T+1 is not a settlement day, move to next settlement day.
    pub fn settlement_date(&self, trade_date: NaiveDate) -> NaiveDate {
        let mut settle = trade_date + chrono::Duration::days(1);
        while !self.is_settlement_day(settle) {
            settle += chrono::Duration::days(1);
        }
        settle
    }

    /// Get the next trading day after a given date.
    pub fn next_trading_day(&self, date: NaiveDate) -> NaiveDate {
        let mut next = date + chrono::Duration::days(1);
        while !self.is_trading_day(next) {
            next += chrono::Duration::days(1);
        }
        next
    }

    /// Get the previous trading day before a given date.
    pub fn prev_trading_day(&self, date: NaiveDate) -> NaiveDate {
        let mut prev = date - chrono::Duration::days(1);
        while !self.is_trading_day(prev) {
            prev -= chrono::Duration::days(1);
        }
        prev
    }
}

/// Product type for Indian equities: Delivery (CNC) vs Intraday (MIS).
///
/// This is the *broker contract-note* / execution product taxonomy
/// (Delivery / Intraday / CoverOrder / BracketOrder); it is deliberately
/// distinct from the journal crate's contract-note `ProductType`, and both are
/// distinct from any future SEBI retail product segmentation (Tier 2A).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Product {
    /// Cash and Carry / Delivery - results in Demat credit/debit on T+1
    Delivery,
    /// Margin Intraday Square-off - must be closed same day, no Demat delivery
    Intraday,
    /// Cover Order (Market + SL)
    CoverOrder,
    /// Bracket Order (Target + SL)
    BracketOrder,
}

impl Product {
    /// Check if this product requires Demat settlement (T+1).
    pub fn requires_settlement(&self) -> bool {
        matches!(self, Product::Delivery)
    }
}

/// A settlement instruction for a single trade.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementInstruction {
    pub trade_id: String,
    pub instrument: InstrumentId,
    pub side: PositionSide,
    pub quantity: Quantity,
    pub price: Price,
    pub product: Product,
    pub trade_date: NaiveDate,
    pub settle_date: NaiveDate,
    pub account_id: String,
    pub cash_flow: Money, // Negative = pay, Positive = receive
}

/// Unsettled position tracking (securities pending Demat credit/debit).
#[derive(Debug, Clone)]
pub struct UnsettledPosition {
    pub instrument: InstrumentId,
    pub side: PositionSide,
    pub quantity: Quantity,
    pub settle_date: NaiveDate,
    pub trade_id: String,
}

/// Demat holding state: free (available) vs blocked (pending settlement).
#[derive(Debug, Clone, Default)]
pub struct DematHoldings {
    /// Free holdings available for trading/transfer
    pub free: HashMap<InstrumentId, Quantity>,
    /// Blocked holdings pending settlement (T+1 delivery)
    pub blocked: HashMap<InstrumentId, Quantity>,
    /// Unsettled receivables (cash/securities to be received on settle_date)
    pub unsettled_receivables: Vec<UnsettledPosition>,
    /// Unsettled payables (cash/securities to be paid on settle_date)
    pub unsettled_payables: Vec<UnsettledPosition>,
}

impl DematHoldings {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add free holdings (e.g., after settlement completes).
    pub fn add_free(&mut self, instrument: InstrumentId, qty: Quantity) {
        let current = self
            .free
            .get(&instrument)
            .copied()
            .unwrap_or(Quantity::ZERO);
        self.free
            .insert(instrument, Quantity::new(current.value() + qty.value()));
    }

    /// Remove free holdings (e.g., for sell order).
    pub fn remove_free(&mut self, instrument: &InstrumentId, qty: Quantity) -> Result<(), String> {
        let current = self.free.get(instrument).copied().unwrap_or(Quantity::ZERO);
        if current.value() < qty.value() {
            return Err("Insufficient free holdings".to_string());
        }
        self.free.insert(
            instrument.clone(),
            Quantity::new(current.value() - qty.value()),
        );
        Ok(())
    }

    /// Block holdings for pending settlement (buy trade executed).
    pub fn block_for_settlement(
        &mut self,
        instrument: InstrumentId,
        qty: Quantity,
        settle_date: NaiveDate,
        trade_id: String,
    ) {
        let current = self
            .blocked
            .get(&instrument)
            .copied()
            .unwrap_or(Quantity::ZERO);
        self.blocked.insert(
            instrument.clone(),
            Quantity::new(current.value() + qty.value()),
        );
        self.unsettled_receivables.push(UnsettledPosition {
            instrument,
            side: PositionSide::Long,
            quantity: qty,
            settle_date,
            trade_id,
        });
    }

    /// Unblock and move to free after settlement completes.
    pub fn settle_receivable(
        &mut self,
        instrument: &InstrumentId,
        qty: Quantity,
    ) -> Result<(), String> {
        let blocked = self
            .blocked
            .get(instrument)
            .copied()
            .unwrap_or(Quantity::ZERO);
        if blocked.value() < qty.value() {
            return Err("Insufficient blocked holdings to settle".to_string());
        }
        self.blocked.insert(
            instrument.clone(),
            Quantity::new(blocked.value() - qty.value()),
        );
        self.add_free(instrument.clone(), qty);

        // Remove from unsettled receivables
        self.unsettled_receivables
            .retain(|u| u.instrument != *instrument || u.quantity.value() != qty.value());
        Ok(())
    }

    /// Get total holdings (free + blocked) for an instrument.
    pub fn total_holdings(&self, instrument: &InstrumentId) -> Quantity {
        let free = self.free.get(instrument).copied().unwrap_or(Quantity::ZERO);
        let blocked = self
            .blocked
            .get(instrument)
            .copied()
            .unwrap_or(Quantity::ZERO);
        Quantity::new(free.value() + blocked.value())
    }

    /// Get free (tradable) holdings for an instrument.
    pub fn free_holdings(&self, instrument: &InstrumentId) -> Quantity {
        self.free.get(instrument).copied().unwrap_or(Quantity::ZERO)
    }
}

/// Result of processing fill-based settlements.
#[derive(Debug, Clone)]
pub struct FillSettlementResult {
    pub settled_count: usize,
    pub failed_count: usize,
    pub cash_flow: Money,
    pub securities_moved: Vec<InstrumentId>,
    pub errors: Vec<String>,
}

impl Default for FillSettlementResult {
    fn default() -> Self {
        Self {
            settled_count: 0,
            failed_count: 0,
            cash_flow: Money::inr(Decimal::ZERO),
            securities_moved: Vec::new(),
            errors: Vec::new(),
        }
    }
}

impl FillSettlementResult {
    pub fn is_successful(&self) -> bool {
        self.failed_count == 0
    }
}

/// Settlement engine for T+1 Demat delivery.
pub struct SettlementEngine {
    calendar: ClearingCalendar,
}

impl Default for SettlementEngine {
    fn default() -> Self {
        Self::new(ClearingCalendar::new())
    }
}

impl SettlementEngine {
    pub fn new(calendar: ClearingCalendar) -> Self {
        Self { calendar }
    }

    /// Generate settlement instructions from a fill.
    pub fn create_instruction(&self, fill: &Fill, account_id: String) -> SettlementInstruction {
        let trade_date = fill.timestamp.date_naive();
        let settle_date = if fill.product.requires_settlement() {
            self.calendar.settlement_date(trade_date)
        } else {
            trade_date // Intraday settles same day
        };

        let cash_flow = if fill.side == PositionSide::Long {
            // Buyer pays
            Money::inr(-(fill.price.value() * fill.quantity.value()))
        } else {
            // Seller receives
            Money::inr(fill.price.value() * fill.quantity.value())
        };

        SettlementInstruction {
            trade_id: fill.trade_id.clone(),
            instrument: fill.instrument.clone(),
            side: fill.side,
            quantity: fill.quantity,
            price: fill.price,
            product: fill.product,
            trade_date,
            settle_date,
            account_id,
            cash_flow,
        }
    }

    /// Process settlements for a given date.
    /// Moves securities from blocked to free, processes cash flows.
    pub fn process_settlements(
        &self,
        holdings: &mut DematHoldings,
        instructions: &[SettlementInstruction],
        current_date: NaiveDate,
    ) -> FillSettlementResult {
        let mut result = FillSettlementResult::default();

        for instr in instructions {
            if instr.settle_date <= current_date && instr.product.requires_settlement() {
                match self.settle_instruction(holdings, instr) {
                    Ok(cash_delta) => {
                        result.settled_count += 1;
                        result.cash_flow = Money::inr(result.cash_flow.amount + cash_delta.amount);
                        result.securities_moved.push(instr.instrument.clone());
                    }
                    Err(e) => {
                        result.failed_count += 1;
                        result
                            .errors
                            .push(format!("Trade {}: {}", instr.trade_id, e));
                    }
                }
            }
        }

        result
    }

    /// Settle a single instruction.
    fn settle_instruction(
        &self,
        holdings: &mut DematHoldings,
        instr: &SettlementInstruction,
    ) -> Result<Money, String> {
        if instr.side == PositionSide::Long {
            // Buyer: receive securities, cash already blocked/paid
            holdings.settle_receivable(&instr.instrument, instr.quantity)?;
        } else {
            // Seller: deliver securities (already blocked at trade time)
            // For sellers, we track the delivery obligation
            holdings.remove_free(&instr.instrument, instr.quantity)?;
        }
        Ok(instr.cash_flow)
    }
}

/// A fill event from the broker/execution layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fill {
    pub trade_id: String,
    pub instrument: InstrumentId,
    pub side: PositionSide,
    pub quantity: Quantity,
    pub price: Price,
    pub product: Product,
    pub timestamp: DateTime<Utc>,
    pub account_id: String,
}

/// A holding of an instrument at a given side and quantity.
///
/// The aggregate carries its own PnL bookkeeping: `realized_pnl` accumulates
/// through [`AccountState::match_lots`] and `updated_at` records the last
/// transition time *injected by the caller* — never minted internally, so the
/// same fill stream replays byte-identically.
#[derive(Debug, Clone)]
pub struct HeldPosition {
    pub instrument: InstrumentId,
    pub side: PositionSide,
    pub quantity: Quantity,
    pub entry_price: Price,
    /// FIFO lot tracking: entry lot IDs for realized PnL computation
    pub entry_lot_ids: Vec<String>,
    /// Entry date for T+1 settlement and tax holding-period tracking
    pub entry_date: NaiveDate,
    /// Realized PnL accumulated for this position (INR).
    pub realized_pnl: Money,
    /// Last update timestamp, injected by the caller. Defaults to the Unix
    /// epoch meaning "not yet stamped".
    pub updated_at: DateTime<Utc>,
}

/// Deterministic epoch date (the library has no clock, so callers that know
/// the real entry date must override via [`HeldPosition::with_entry_date`]).
const DEFAULT_ENTRY_DATE: &str = "2026-01-01";

/// A FIFO tax lot: the canonical cost-basis record for strict FIFO settlement
/// accounting.
///
/// Superset of the deleted `Fitolot` / `CostBasisLot` stubs and of the
/// facade repository's former private copy: the *one* shared definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaxLot {
    pub lot_id: String,
    pub account_id: vyapaar_core::AccountId,
    pub instrument: InstrumentId,
    pub side: PositionSide,
    pub entry_price: Price,
    pub quantity: Quantity,
    pub remaining_quantity: Quantity,
    pub entry_date: NaiveDate,
    pub created_at: DateTime<Utc>,
}

impl TaxLot {
    /// True once the lot has been fully matched away.
    pub fn is_exhausted(&self) -> bool {
        self.remaining_quantity.value() <= Decimal::ZERO
    }
}

/// Realized gain/loss details for a matched tax lot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchedLot {
    pub lot_id: String,
    pub matched_quantity: Quantity,
    pub entry_price: Price,
    pub exit_price: Price,
    pub realized_pnl: Money,
    pub holding_period_days: i64,
    pub is_long_term: bool,
}

/// The matching input for [`AccountState::match_lots`]: the slice of a fill
/// report the FIFO engine actually needs (the projection repositories use).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LotFill {
    /// Traded instrument.
    pub instrument: InstrumentId,
    /// Order side of the fill (Buy closes Shorts, Sell closes Longs).
    pub side: OrderSide,
    /// Exact execution price.
    pub price: Price,
    /// Executed quantity.
    pub quantity: Quantity,
    /// Execution timestamp injected by the caller.
    pub timestamp: DateTime<Utc>,
}

impl HeldPosition {
    pub fn new(
        instrument_id: InstrumentId,
        side: PositionSide,
        qty: Quantity,
        price: Price,
    ) -> Self {
        Self {
            instrument: instrument_id,
            side,
            quantity: qty,
            entry_price: price,
            entry_lot_ids: vec![],
            entry_date: NaiveDate::parse_from_str(DEFAULT_ENTRY_DATE, "%Y-%m-%d")
                .expect("valid constant date"),
            realized_pnl: Money::inr(Decimal::ZERO),
            updated_at: DateTime::<Utc>::UNIX_EPOCH,
        }
    }

    /// Override the deterministic epoch entry date with the caller-known date.
    pub fn with_entry_date(mut self, entry_date: NaiveDate) -> Self {
        self.entry_date = entry_date;
        self
    }

    /// Override the default `updated_at` epoch stamp.
    pub fn with_updated_at(mut self, updated_at: DateTime<Utc>) -> Self {
        self.updated_at = updated_at;
        self
    }
}

/// Corporate action type for Indian equities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CorporateAction {
    /// Stock split: e.g., 1:2 means 1 share becomes 2 shares, price halves
    /// ratio_numerator : ratio_denominator (e.g., 2:1 split = 2/1)
    Split {
        ratio_numerator: u64,
        ratio_denominator: u64,
        ex_date: NaiveDate,
    },
    /// Bonus issue: e.g., 1:1 bonus means 1 free share for every 1 held
    /// Similar to split but no price change on ex-date (price adjusts by market)
    Bonus {
        ratio_numerator: u64,
        ratio_denominator: u64,
        ex_date: NaiveDate,
    },
    /// Cash dividend per share (in paise/minor units)
    /// Does not affect position quantity, but generates cash income
    Dividend {
        amount_per_share: i64, // minor units (paise)
        ex_date: NaiveDate,
        record_date: NaiveDate,
        payment_date: NaiveDate,
    },
    /// Demerger/spin-off: new instrument received proportionally
    /// allocation_ratio: how many new shares per old share (e.g., 1:1 = 1/1)
    Demerger {
        new_instrument: InstrumentId,
        allocation_numerator: u64,
        allocation_denominator: u64,
        cost_basis_allocation_pct: Decimal, // % of original cost basis allocated to new instrument
        ex_date: NaiveDate,
    },
}

impl CorporateAction {
    /// Get the ex-date of the corporate action
    pub fn ex_date(&self) -> NaiveDate {
        match self {
            CorporateAction::Split { ex_date, .. }
            | CorporateAction::Bonus { ex_date, .. }
            | CorporateAction::Dividend { ex_date, .. }
            | CorporateAction::Demerger { ex_date, .. } => *ex_date,
        }
    }
}

/// Adjust position for a stock split.
/// Quantity multiplies by ratio, entry price divides by ratio.
/// FIFO lot quantities and prices are adjusted proportionally.
pub fn adjust_for_split(pos: &mut HeldPosition, ratio_numerator: u64, ratio_denominator: u64) {
    if ratio_numerator == 0 || ratio_denominator == 0 {
        return; // Invalid ratio, no adjustment
    }

    let ratio = Decimal::from(ratio_numerator) / Decimal::from(ratio_denominator);

    // Adjust quantity
    pos.quantity = Quantity::new(pos.quantity.value() * ratio);

    // Adjust entry price (inverse of quantity ratio)
    let new_entry_price = pos.entry_price.value() / ratio;
    pos.entry_price = Price::new(new_entry_price);

    // FIFO lot IDs remain the same (they track the same lots, just adjusted)
    // Note: In a full implementation with individual lot tracking, each lot would be adjusted
}

/// Adjust position for a bonus issue.
/// Similar to split: quantity increases, cost basis per share decreases.
/// The key difference: bonus shares are received free, so total cost basis remains the same.
pub fn adjust_for_bonus(pos: &mut HeldPosition, ratio_numerator: u64, ratio_denominator: u64) {
    if ratio_numerator == 0 || ratio_denominator == 0 {
        return;
    }

    let ratio = Decimal::from(ratio_numerator) / Decimal::from(ratio_denominator);

    // Adjust quantity (increase by bonus ratio)
    pos.quantity = Quantity::new(pos.quantity.value() * (Decimal::ONE + ratio));

    // Adjust entry price to keep total cost basis constant
    // New cost basis per share = old_total_cost / new_total_qty
    // = (old_price * old_qty) / (old_qty * (1 + ratio))
    // = old_price / (1 + ratio)
    let new_entry_price = pos.entry_price.value() / (Decimal::ONE + ratio);
    pos.entry_price = Price::new(new_entry_price);

    // FIFO lot IDs remain the same
}

/// Record a dividend payment.
/// Dividends don't affect position quantity or cost basis.
/// Returns the dividend income amount for the given position quantity.
pub fn record_dividend(pos: &HeldPosition, amount_per_share: i64) -> Money {
    // Dividend income = amount_per_share * quantity (in minor units)
    Money::inr(Decimal::from(amount_per_share) * pos.quantity.value())
}

/// Adjust position for a demerger/spin-off.
/// Creates a new position for the spun-off instrument.
/// Original position's cost basis is split between old and new instruments.
pub fn adjust_for_demerger(
    pos: &HeldPosition,
    new_instrument: InstrumentId,
    allocation_numerator: u64,
    allocation_denominator: u64,
    cost_basis_allocation_pct: Decimal,
) -> Option<HeldPosition> {
    if allocation_numerator == 0 || allocation_denominator == 0 {
        return None;
    }

    let allocation_ratio =
        Decimal::from(allocation_numerator) / Decimal::from(allocation_denominator);

    // Calculate quantity for new instrument
    let new_qty = pos.quantity.value() * allocation_ratio;
    if new_qty <= Decimal::ZERO {
        return None;
    }

    // Allocate cost basis: new instrument gets a percentage of original cost basis
    // Old instrument retains (1 - allocation_pct) of cost basis
    let new_entry_price = pos.entry_price.value() * cost_basis_allocation_pct / allocation_ratio;

    // Create new position for demerged entity
    Some(HeldPosition {
        instrument: new_instrument,
        side: pos.side,
        quantity: Quantity::new(new_qty),
        entry_price: Price::new(new_entry_price),
        entry_lot_ids: pos
            .entry_lot_ids
            .iter()
            .map(|id| format!("DEMERGER_{id}"))
            .collect(),
        entry_date: pos.entry_date,
        realized_pnl: Money::inr(Decimal::ZERO),
        updated_at: pos.updated_at,
    })
}

/// Apply a corporate action to a position.
/// Returns dividend income if applicable, and new position for demerger.
pub fn apply_corporate_action(
    pos: &mut HeldPosition,
    action: &CorporateAction,
) -> (Money, Option<HeldPosition>) {
    match action {
        CorporateAction::Split {
            ratio_numerator,
            ratio_denominator,
            ..
        } => {
            adjust_for_split(pos, *ratio_numerator, *ratio_denominator);
            (Money::inr(Decimal::ZERO), None)
        }
        CorporateAction::Bonus {
            ratio_numerator,
            ratio_denominator,
            ..
        } => {
            adjust_for_bonus(pos, *ratio_numerator, *ratio_denominator);
            (Money::inr(Decimal::ZERO), None)
        }
        CorporateAction::Dividend {
            amount_per_share, ..
        } => {
            let income = record_dividend(pos, *amount_per_share);
            (income, None)
        }
        CorporateAction::Demerger {
            new_instrument,
            allocation_numerator,
            allocation_denominator,
            cost_basis_allocation_pct,
            ..
        } => {
            let new_pos = adjust_for_demerger(
                pos,
                new_instrument.clone(),
                *allocation_numerator,
                *allocation_denominator,
                *cost_basis_allocation_pct,
            );
            // Update original position's entry price for remaining cost basis
            // This is a simplified adjustment
            let remaining_cost_basis =
                pos.entry_price.value() * (Decimal::ONE - *cost_basis_allocation_pct);
            // Adjust original entry price for the reduced cost basis
            // This is approximate; full implementation would track per-lot
            pos.entry_price = Price::new(remaining_cost_basis);
            (Money::inr(Decimal::ZERO), new_pos)
        }
    }
}

/// Apply all corporate actions that occurred before the given date to a position.
/// Corporate actions are applied in chronological order.
pub fn apply_corporate_actions_to_position(
    pos: &mut HeldPosition,
    actions: &[CorporateAction],
    as_of: NaiveDate,
) -> Money {
    let mut total_dividend_income = Money::inr(Decimal::ZERO);

    for action in actions {
        if action.ex_date() <= as_of {
            let (dividend, _) = apply_corporate_action(pos, action);
            total_dividend_income = Money::inr(total_dividend_income.amount + dividend.amount);
        }
    }

    total_dividend_income
}

/// Apply corporate actions to all positions in an account.
pub fn apply_corporate_actions_to_account(
    account: &mut AccountState,
    actions: &[CorporateAction],
    as_of: NaiveDate,
) -> Money {
    let mut total_dividend_income = Money::inr(Decimal::ZERO);

    for pos in &mut account.positions {
        total_dividend_income = Money::inr(
            total_dividend_income.amount
                + apply_corporate_actions_to_position(pos, actions, as_of).amount,
        );
    }

    total_dividend_income
}

/// State of a multi-account portfolio.
#[derive(Debug, Clone)]
pub struct PortfolioState {
    pub accounts: BTreeMap<String, AccountState>,
    pub base_currency: Currency,
}

impl PortfolioState {
    pub fn new(base_currency: Currency) -> Self {
        Self {
            accounts: BTreeMap::new(),
            base_currency,
        }
    }
}

/// Per-account aggregate state: cash, held positions, and the canonical FIFO
/// tax-lot book.
#[derive(Debug, Clone)]
pub struct AccountState {
    pub cash: Money,
    pub positions: Vec<HeldPosition>,
    /// Canonical FIFO tax lots backing the positions.
    pub lots: Vec<TaxLot>,
}

impl AccountState {
    pub fn new(currency: Currency, positions: Vec<HeldPosition>) -> Self {
        Self {
            cash: Money::new(Decimal::ZERO, currency),
            positions,
            lots: Vec::new(),
        }
    }

    /// Record a tax lot into the aggregate: append it to the lot book and
    /// create-or-merge the backing [`HeldPosition`].
    ///
    /// This is the aggregate-level operation the repository layer used to
    /// perform inline; keeping it here lets the in-memory repository, a future
    /// PostgreSQL projection, and tests share one implementation.
    pub fn record_lot(&mut self, lot: &TaxLot) {
        self.lots.push(lot.clone());
        let instrument = lot.instrument.clone();
        match self
            .positions
            .iter_mut()
            .find(|p| p.instrument == instrument)
        {
            Some(pos) => {
                pos.quantity = Quantity::new(pos.quantity.value() + lot.quantity.value());
                pos.updated_at = lot.created_at;
            }
            None => {
                self.positions.push(
                    HeldPosition::new(instrument, lot.side, lot.quantity, lot.entry_price)
                        .with_entry_date(lot.entry_date)
                        .with_updated_at(lot.created_at),
                );
            }
        }
    }

    /// Match an incoming fill against open tax lots using strict FIFO
    /// matching, roll the realized PnL onto the matching [`HeldPosition`],
    /// and return the per-lot matching details.
    ///
    /// Only opposing sides match: a `Sell` fill closes `Long` lots and a `Buy`
    /// fill closes `Short` lots (Indian equity delivery semantics). Matched
    /// quantity never exceeds the fill; unmatched remainder is the caller's
    /// concern (a fresh opening leg).
    pub fn match_lots(&mut self, fill: &LotFill) -> Vec<MatchedLot> {
        let mut matched_lots = Vec::new();
        let mut remaining_fill = fill.quantity.value();
        let fill_date = fill.timestamp.date_naive();

        for lot in self.lots.iter_mut() {
            if lot.instrument != fill.instrument || lot.is_exhausted() {
                continue;
            }

            // Only match against opposing side (Long vs Sell, Short vs Buy)
            let is_closing = matches!(
                (lot.side, fill.side),
                (PositionSide::Long, OrderSide::Sell) | (PositionSide::Short, OrderSide::Buy)
            );
            if !is_closing {
                continue;
            }

            let lot_avail = lot.remaining_quantity.value();
            let match_qty_dec = remaining_fill.min(lot_avail);
            if match_qty_dec <= Decimal::ZERO {
                continue;
            }

            let entry_p = lot.entry_price.value();
            let exit_p = fill.price.value();
            let pnl_unit = match lot.side {
                PositionSide::Long => exit_p - entry_p,
                PositionSide::Short => entry_p - exit_p,
            };
            let realized_pnl = Money::inr(pnl_unit * match_qty_dec);

            let holding_days = (fill_date - lot.entry_date).num_days();
            let is_long_term = holding_days > 365;

            matched_lots.push(MatchedLot {
                lot_id: lot.lot_id.clone(),
                matched_quantity: Quantity::new(match_qty_dec),
                entry_price: lot.entry_price,
                exit_price: fill.price,
                realized_pnl,
                holding_period_days: holding_days,
                is_long_term,
            });

            lot.remaining_quantity = Quantity::new(lot_avail - match_qty_dec);
            remaining_fill -= match_qty_dec;

            if remaining_fill <= Decimal::ZERO {
                break;
            }
        }

        // Roll the aggregate's realized PnL and quantity onto the position.
        let total_matched: Decimal = matched_lots
            .iter()
            .map(|m| m.matched_quantity.value())
            .sum();
        if total_matched > Decimal::ZERO {
            let total_realized: Decimal = matched_lots.iter().map(|m| m.realized_pnl.amount).sum();
            if let Some(pos) = self
                .positions
                .iter_mut()
                .find(|p| p.instrument == fill.instrument)
            {
                pos.quantity =
                    Quantity::new((pos.quantity.value() - total_matched).max(Decimal::ZERO));
                pos.realized_pnl = Money::inr(pos.realized_pnl.amount + total_realized);
                pos.updated_at = fill.timestamp;
            }
        }

        matched_lots
    }
}

/// Market marks (last price) for instruments.
#[derive(Debug, Clone)]
pub struct Marks {
    pub marks: HashMap<InstrumentId, Price>,
}

impl Default for Marks {
    fn default() -> Self {
        Self::new()
    }
}

impl Marks {
    pub fn new() -> Self {
        Self {
            marks: HashMap::new(),
        }
    }

    pub fn with(mut self, instrument: InstrumentId, price: Price) -> Self {
        self.marks.insert(instrument, price);
        self
    }
}

/// Portfolio valuation result.
#[derive(Debug, Clone)]
pub struct PortfolioSnapshot {
    pub totals: Money,
    pub breakdown: BTreeMap<String, Money>,
    pub unrealized_pnl: Money,
    pub realized_pnl: Money,
}

/// Compute the portfolio valuation given state and market marks.
///
/// Pure and deterministic: no clock, no I/O, byte-identical output for equal
/// inputs. Realized PnL comes from every held position (a flat position's
/// realized PnL still counts); market value and unrealized PnL come only from
/// open positions against their marks (unmarked positions value at zero).
pub fn value(state: &PortfolioState, marks: &Marks) -> PortfolioSnapshot {
    let mut totals = Money::inr(Decimal::ZERO);
    let mut total_unrealized = Money::inr(Decimal::ZERO);
    let mut total_realized = Money::inr(Decimal::ZERO);
    let mut breakdown: BTreeMap<String, Money> = BTreeMap::new();

    for account in state.accounts.values() {
        for pos in &account.positions {
            let market_price = marks
                .marks
                .get(&pos.instrument)
                .copied()
                .unwrap_or(Price::ZERO);
            let market_value = Money::inr(market_price.value() * pos.quantity.value());
            let cost_basis = Money::inr(pos.entry_price.value() * pos.quantity.value());

            totals = Money::inr(totals.amount + market_value.amount);
            total_unrealized =
                Money::inr(total_unrealized.amount + market_value.amount - cost_basis.amount);
            total_realized = Money::inr(total_realized.amount + pos.realized_pnl.amount);

            *breakdown
                .entry(pos.instrument.symbol.to_string())
                .or_insert(Money::inr(Decimal::ZERO)) = market_value;
        }
    }

    PortfolioSnapshot {
        totals,
        breakdown,
        unrealized_pnl: total_unrealized,
        realized_pnl: total_realized,
    }
}

/// Apply a fill to a position, updating quantity and tracking realized PnL.
/// Uses FIFO: first-in-first-out lot elimination.
pub fn apply_fill(
    pos: &mut HeldPosition,
    _side: PositionSide,
    fill_qty: u64,
    fill_price: Price,
) -> Money {
    let fill_qty_dec = Decimal::from(fill_qty);
    let entry_price_dec = pos.entry_price.value();
    let fill_price_dec = fill_price.value();

    match pos.side {
        PositionSide::Long => {
            if fill_qty_dec <= pos.quantity.value() {
                // Partial fill: reduce quantity, compute PnL on the filled portion
                let pnl = (fill_price_dec - entry_price_dec) * fill_qty_dec;
                pos.quantity = Quantity::new(pos.quantity.value() - fill_qty_dec);
                Money::inr(pnl)
            } else {
                // Full close + reverse: the existing long is closed entirely
                // (realize PnL on ONLY that closed quantity), and a new SHORT
                // leg is opened for the overfilled remainder at the fill
                // price. The remainder is a fresh unrealized position, not
                // already-realized PnL.
                let closed_qty = pos.quantity.value();
                let pnl_on_close = (fill_price_dec - entry_price_dec) * closed_qty;
                let reverse_qty = fill_qty_dec - closed_qty;
                pos.side = PositionSide::Short;
                pos.quantity = Quantity::new(reverse_qty);
                pos.entry_price = fill_price;
                pos.entry_lot_ids.clear();
                Money::inr(pnl_on_close)
            }
        }
        PositionSide::Short => {
            if fill_qty_dec <= pos.quantity.value() {
                // Short realized PnL is symmetric to Long: profit when the
                // buy-back (fill_price) is BELOW the entry (short-sale) price.
                let pnl = (entry_price_dec - fill_price_dec) * fill_qty_dec;
                pos.quantity = Quantity::new(pos.quantity.value() - fill_qty_dec);
                Money::inr(pnl)
            } else {
                // Full close + reverse: symmetric to the Long case above.
                let closed_qty = pos.quantity.value();
                let pnl_on_close = (entry_price_dec - fill_price_dec) * closed_qty;
                let reverse_qty = fill_qty_dec - closed_qty;
                pos.side = PositionSide::Long;
                pos.quantity = Quantity::new(reverse_qty);
                pos.entry_price = fill_price;
                pos.entry_lot_ids.clear();
                Money::inr(pnl_on_close)
            }
        }
    }
}

/// Apply a fill to an account position.
pub fn apply_fill_to_account(
    account: &mut AccountState,
    instrument_id: InstrumentId,
    side: PositionSide,
    fill_qty: u64,
    fill_price: Price,
) -> Money {
    let pos_idx = account
        .positions
        .iter()
        .position(|p| p.instrument == instrument_id);

    match pos_idx {
        Some(idx) => {
            let mut pos = account.positions[idx].clone();
            let realized = apply_fill(&mut pos, side, fill_qty, fill_price);
            pos.realized_pnl = Money::inr(pos.realized_pnl.amount + realized.amount);
            account.positions[idx] = pos;
            realized
        }
        None => {
            let pos = HeldPosition::new(
                instrument_id,
                side,
                Quantity::new(Decimal::from(fill_qty)),
                fill_price,
            );
            account.positions.push(pos);
            Money::inr(Decimal::ZERO)
        }
    }
}

/// Build a `HeldPosition` from a fill event, initializing entry price and FIFO lot tracking.
pub fn init_position(
    instrument_id: InstrumentId,
    side: PositionSide,
    qty: u64,
    price: Price,
) -> HeldPosition {
    HeldPosition::new(
        instrument_id,
        side,
        Quantity::new(Decimal::from(qty)),
        price,
    )
}

/// A trade execution record with trade and settlement dates.
/// T+1 settlement: trades settle one business day after the trade date.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub trade_date: NaiveDate,
    /// Settlement date (trade_date + 1 business day)
    pub settle_date: NaiveDate,
    pub instrument: InstrumentId,
    pub side: PositionSide,
    pub quantity: u64,
    pub price: Price,
    /// Minor units (paise) for cash flow
    pub cash_flow: i64,
}

/// Settlement result for a T+1 settlement operation (trade-based settlement).
#[derive(Debug, Clone)]
pub struct TradeSettlementResult {
    /// Accounts affected by the settlement
    pub affected_accounts: Vec<String>,
    /// Cash moved between accounts (positive = net cash inflow to account)
    pub cash_movements: BTreeMap<String, Money>,
    /// Positions changed (securities moved)
    pub position_changes: BTreeMap<String, i64>, // net quantity change per instrument (positive = long increase)
    /// Whether settlement was successful
    pub successful: bool,
    /// Error message if settlement failed
    pub error: Option<String>,
}

/// Perform T+1 settlement on all trades that have reached their settle_date.
/// Moves cash and securities between accounts to reflect settled trades.
pub fn settle_trades(
    state: &mut PortfolioState,
    trades: &[Trade],
    current_date: NaiveDate,
) -> TradeSettlementResult {
    let mut affected_accounts: Vec<String> = Vec::new();
    let mut cash_movements: BTreeMap<String, Money> = BTreeMap::new();
    let mut position_changes: BTreeMap<String, i64> = BTreeMap::new();
    let successful = true;
    let error: Option<String> = None;

    for trade in trades {
        // Check if this trade's settle_date has arrived
        if trade.settle_date <= current_date {
            // Find the account(s) that have positions in this instrument
            for (account_name, account) in &mut state.accounts {
                affected_accounts.push(account_name.clone());

                // If account has a position in this instrument, adjust it
                let pos_idx = account
                    .positions
                    .iter()
                    .position(|p| p.instrument == trade.instrument);

                if let Some(idx) = pos_idx {
                    if trade.side == PositionSide::Long {
                        // Reduce long position (someone is selling to this account's buyer)
                        *position_changes
                            .entry(trade.instrument.symbol.to_string())
                            .or_insert(0) -= i64::try_from(trade.quantity).unwrap_or(i64::MAX);

                        // Adjust cash: add proceeds
                        let proceeds =
                            Money::inr(trade.price.value() * Decimal::from(trade.quantity));
                        let cash_entry = cash_movements
                            .entry(account_name.clone())
                            .or_insert_with(|| Money::inr(Decimal::ZERO));
                        *cash_entry = Money::inr(cash_entry.amount + proceeds.amount);

                        // Reduce position quantity
                        if let Some(p) = account.positions.get_mut(idx) {
                            p.quantity =
                                Quantity::new(p.quantity.value() - Decimal::from(trade.quantity));
                        }
                    } else if trade.side == PositionSide::Short {
                        // Cover short position
                        *position_changes
                            .entry(trade.instrument.symbol.to_string())
                            .or_insert(0) += i64::try_from(trade.quantity).unwrap_or(i64::MAX);

                        // Adjust cash: subtract buy-in cost
                        let cost = Money::inr(trade.price.value() * Decimal::from(trade.quantity));
                        let cash_entry = cash_movements
                            .entry(account_name.clone())
                            .or_insert_with(|| Money::inr(Decimal::ZERO));
                        *cash_entry = Money::inr(cash_entry.amount - cost.amount);

                        // Remove or reduce short position
                        if let Some(p) = account.positions.get_mut(idx) {
                            p.quantity =
                                Quantity::new(p.quantity.value() - Decimal::from(trade.quantity));
                            if p.quantity.value() <= Decimal::ZERO {
                                account.positions.remove(idx);
                            }
                        }
                    }
                } else {
                    // No existing position - this is a new trade settlement
                    // Cash flow for buying/selling
                    if trade.side == PositionSide::Long {
                        // Buyer pays cash
                        let cost = Money::inr(trade.price.value() * Decimal::from(trade.quantity));
                        let cash_entry = cash_movements
                            .entry(account_name.clone())
                            .or_insert_with(|| Money::inr(Decimal::ZERO));
                        *cash_entry = Money::inr(cash_entry.amount - cost.amount);
                    } else {
                        // Seller receives cash
                        let proceeds =
                            Money::inr(trade.price.value() * Decimal::from(trade.quantity));
                        let cash_entry = cash_movements
                            .entry(account_name.clone())
                            .or_insert_with(|| Money::inr(Decimal::ZERO));
                        *cash_entry = Money::inr(cash_entry.amount + proceeds.amount);
                    }

                    // Create position if buying
                    if trade.side == PositionSide::Long {
                        account.positions.push(HeldPosition::new(
                            trade.instrument.clone(),
                            PositionSide::Long,
                            Quantity::new(Decimal::from(trade.quantity)),
                            trade.price,
                        ));
                    }
                }
            }
        }
    }

    TradeSettlementResult {
        affected_accounts,
        cash_movements,
        position_changes,
        successful,
        error,
    }
}

/// Capital gains computation for a single lot.
/// Uses FIFO cost basis method with holding period awareness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapitalGainType {
    ShortTerm, // Held <= 365 days
    LongTerm,  // Held > 365 days
}

/// Compute capital gains for a sold portion of a position.
/// Uses FIFO: first-in-first-out lot elimination.
/// Returns realized PnL and the capital gain type (short/long term).
pub fn compute_capital_gains(
    pos: &HeldPosition,
    sale_qty: u64,
    sale_price: Price,
    sale_date: NaiveDate,
) -> (Money, CapitalGainType) {
    let sale_qty_dec = Decimal::from(sale_qty);
    let sale_price_dec = sale_price.value();
    let entry_price_dec = pos.entry_price.value();
    let holding_period = sale_date.signed_duration_since(pos.entry_date).num_days();

    let mut capital_gain_type = CapitalGainType::ShortTerm;

    // Determine holding period category
    if holding_period > 365 {
        capital_gain_type = CapitalGainType::LongTerm;
    }

    // Compute PnL: proceeds - cost basis for the sold quantity
    let pnl_amount = (sale_price_dec - entry_price_dec) * sale_qty_dec;
    let realized_pnl = Money::inr(pnl_amount);

    (realized_pnl, capital_gain_type)
}

/// Tax summary for a portfolio period.
#[derive(Debug, Clone)]
pub struct TaxSummary {
    /// Total realized PnL
    pub total_realized_pnl: Money,
    /// Short-term capital gains
    pub short_term_pnl: Money,
    /// Long-term capital gains
    pub long_term_pnl: Money,
    /// Total securities sold (quantity)
    pub total_sold_qty: u64,
    /// Period covered (from to)
    pub period_start: NaiveDate,
    pub period_end: NaiveDate,
}

impl TaxSummary {
    /// Generate a tax summary from a series of realized fills.
    /// Uses FIFO cost basis with holding period awareness.
    ///
    /// # Corporate Actions
    /// Corporate actions (splits, bonuses, dividends, demergers) that occurred
    /// before the fill date are applied to positions before computing FIFO cost basis.
    /// This ensures accurate cost basis tracking after corporate actions.
    pub fn from_fills(
        fills: &[(
            &AccountState,
            InstrumentId,
            PositionSide,
            u64,
            Price,
            NaiveDate,
        )],
        period_start: NaiveDate,
        period_end: NaiveDate,
        corporate_actions: &[CorporateAction],
    ) -> Self {
        let mut total_realized_pnl = Money::inr(Decimal::ZERO);
        let mut short_term_pnl = Money::inr(Decimal::ZERO);
        let mut long_term_pnl = Money::inr(Decimal::ZERO);
        let mut total_sold_qty: u64 = 0;

        for (account, instrument_id, _side, fill_qty, fill_price, fill_date) in fills {
            let pos_idx = account
                .positions
                .iter()
                .position(|p| p.instrument == *instrument_id);

            if let Some(idx) = pos_idx {
                // Create a mutable copy of the position to apply corporate actions
                let mut pos = account.positions[idx].clone();

                // Apply corporate actions that occurred before this fill date
                // This adjusts the position's quantity and entry_price for accurate FIFO cost basis
                let _dividend_income =
                    apply_corporate_actions_to_position(&mut pos, corporate_actions, *fill_date);

                // Compute capital gains using the adjusted position
                let (pnl, gain_type) =
                    compute_capital_gains(&pos, *fill_qty, *fill_price, *fill_date);

                total_realized_pnl = Money::inr(total_realized_pnl.amount + pnl.amount);
                total_sold_qty += *fill_qty;

                match gain_type {
                    CapitalGainType::ShortTerm => {
                        short_term_pnl = Money::inr(short_term_pnl.amount + pnl.amount);
                    }
                    CapitalGainType::LongTerm => {
                        long_term_pnl = Money::inr(long_term_pnl.amount + pnl.amount);
                    }
                }
            }
        }

        TaxSummary {
            total_realized_pnl,
            short_term_pnl,
            long_term_pnl,
            total_sold_qty,
            period_start,
            period_end,
        }
    }
}
