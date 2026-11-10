//! Phase 1 — data model: journal account, trade annotation, day-keyed note,
//! and the derived closed-trade type.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::{Currency, InstrumentId, Money, PositionSide, Price, Quantity};

/// Strongly-typed identifier for a journal account — distinct from this repo's
/// own OMS/portfolio `AccountId` (a journal account is an analytics grouping,
/// not a live-execution account).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct JournalAccountId(pub String);

impl JournalAccountId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for JournalAccountId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Provenance of a journal account — mirrors Vela's distinction between a
/// trading account, a brokerage connection, and a separate journal account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountProvenance {
    /// Manually created by the user.
    Manual,
    /// Imported from an external source (CSV, broker statement, etc.).
    Imported,
    /// Synced from a live broker — still its own analytics ledger, not a
    /// live-execution account.
    BrokerSynced,
}

/// Profit / loss calculation convention. v1 hard-codes FIFO (matching the
/// existing `AccountState::match_lots` engine); the enum is modeled so a
/// second convention can be added later without changing the type shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PnlConvention {
    Fifo,
}

/// A trader's journal account — a grouping/provenance wrapper for analytics,
/// distinct from the live OMS/portfolio `AccountState`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JournalAccount {
    pub id: JournalAccountId,
    pub display_name: String,
    pub provenance: AccountProvenance,
    pub currency: Currency,
    pub starting_balance: Option<Money>,
    pub pnl_convention: PnlConvention,
}

impl JournalAccount {
    pub fn new(
        id: JournalAccountId,
        display_name: impl Into<String>,
        provenance: AccountProvenance,
        currency: Currency,
    ) -> Self {
        Self {
            id,
            display_name: display_name.into(),
            provenance,
            currency,
            starting_balance: None,
            pnl_convention: PnlConvention::Fifo,
        }
    }

    pub fn with_starting_balance(mut self, balance: Money) -> Self {
        self.starting_balance = Some(balance);
        self
    }
}

/// Trade annotation attached per closed round-trip (per `MatchedLot` /
/// derived `ClosedTrade`). `planned_risk` is a genuinely new user-supplied
/// input: R-multiple (realized PnL ÷ planned risk) is undefined without it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TradeAnnotation {
    pub tags: Vec<String>,
    pub rating: Option<u8>,     // star rating (1-5)
    pub review: Option<String>, // long-form text
    pub planned_risk: Option<Money>,
}

impl TradeAnnotation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    pub fn with_rating(mut self, rating: u8) -> Self {
        self.rating = Some(rating);
        self
    }

    pub fn with_review(mut self, review: impl Into<String>) -> Self {
        self.review = Some(review.into());
        self
    }

    pub fn with_planned_risk(mut self, risk: Money) -> Self {
        self.planned_risk = Some(risk);
        self
    }

    /// True if the user has supplied a non-zero planned risk, making R-multiple
    /// computable for this trade.
    pub fn has_risk(&self) -> bool {
        self.planned_risk.is_some_and(|r| !r.amount.is_zero())
    }
}

/// Day-keyed journal note — independent of any specific trade (a note can
/// exist with no trade reference at all).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalNote {
    pub account_id: JournalAccountId,
    pub day: NaiveDate,
    pub text: String,
}

impl JournalNote {
    pub fn new(account_id: JournalAccountId, day: NaiveDate, text: impl Into<String>) -> Self {
        Self {
            account_id,
            day,
            text: text.into(),
        }
    }
}

/// A derived closed trade: one FIFO-matched round-trip leg (a `MatchedLot`
/// from `vyapaar-ledger-portfolio`) plus its user annotation. This is the
/// unit the metrics engine consumes — one row per closed trade with realized
/// PnL, duration, and annotation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClosedTrade {
    /// Correlates back to the matched lot / fill that produced this trade.
    pub trade_id: String,
    pub instrument: InstrumentId,
    /// Opening side (the side of the lot that was closed).
    pub side: PositionSide,
    pub quantity: Quantity,
    pub entry_price: Price,
    pub exit_price: Price,
    /// Realized PnL for this matched lot (INR), gross of fees.
    pub realized_pnl: Money,
    /// Fees paid on this trade (INR), default zero.
    pub fees: Money,
    pub holding_period_days: i64,
    pub is_long_term: bool,
    /// Exit (settlement) date of the closing leg.
    pub exit_date: NaiveDate,
    /// Duration of the trade in milliseconds (optional; supplied by the
    /// caller from fill timestamps when available).
    pub duration_ms: Option<u64>,
    pub annotation: TradeAnnotation,
}

impl ClosedTrade {
    /// Build a closed trade from a portfolio `MatchedLot` plus a user
    /// annotation and the closing leg's exit date. Quantity, entry price and
    /// exit price are taken from the matched lot itself.
    pub fn from_matched_lot(
        lot: &vyapaar_ledger_portfolio::MatchedLot,
        instrument: InstrumentId,
        side: PositionSide,
        exit_date: NaiveDate,
        annotation: TradeAnnotation,
    ) -> Self {
        Self {
            trade_id: lot.lot_id.clone(),
            instrument,
            side,
            quantity: lot.matched_quantity,
            entry_price: lot.entry_price,
            exit_price: lot.exit_price,
            realized_pnl: lot.realized_pnl,
            fees: Money::zero_inr(),
            holding_period_days: lot.holding_period_days,
            is_long_term: lot.is_long_term,
            exit_date,
            duration_ms: None,
            annotation,
        }
    }

    /// Net PnL for this trade: realized PnL minus fees.
    pub fn net_pnl(&self) -> Money {
        Money::inr(self.realized_pnl.amount - self.fees.amount)
    }

    /// True if this trade is a win (net PnL strictly positive).
    pub fn is_win(&self) -> bool {
        self.net_pnl().amount > Decimal::ZERO
    }

    /// True if this trade is a loss (net PnL strictly negative).
    pub fn is_loss(&self) -> bool {
        self.net_pnl().amount < Decimal::ZERO
    }

    /// True if this trade is a breakeven (net PnL exactly zero).
    pub fn is_breakeven(&self) -> bool {
        self.net_pnl().amount == Decimal::ZERO
    }

    /// Gross contribution: net PnL if positive, else zero (used for
    /// gross-profit / profit-factor math).
    pub fn gross_profit(&self) -> Money {
        if self.is_win() {
            self.net_pnl()
        } else {
            Money::zero_inr()
        }
    }

    /// Gross loss contribution: absolute net PnL if negative, else zero.
    pub fn gross_loss(&self) -> Money {
        if self.is_loss() {
            Money::inr(-self.net_pnl().amount)
        } else {
            Money::zero_inr()
        }
    }

    /// R-multiple: realized PnL ÷ planned risk. `None` when no planned risk
    /// is set or the planned risk is zero.
    pub fn realized_r(&self) -> Option<Decimal> {
        let risk = self.annotation.planned_risk?;
        if risk.amount.is_zero() {
            return None;
        }
        Some(self.net_pnl().amount / risk.amount)
    }
}
