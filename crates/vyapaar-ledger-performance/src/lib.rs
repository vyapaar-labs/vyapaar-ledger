//! # व्यापार・परफॉरमेंस vyapaar-ledger-performance
//!
//! Pure trade-performance **journal** — a distinct sibling of
//! [`vyapaar-ledger-journal`](crate) (the double-entry accounting ledger) and
//! [`vyapaar-ledger-portfolio`](vyapaar_ledger_portfolio) (FIFO tax-lot
//! valuation). This crate owns the trader's *performance/annotations journal*:
//! trade annotations (tags, star rating, review, planned risk), day-keyed
//! notes, Vela-equivalent performance metrics, a composite edge score, a daily
//! P&L calendar, and account-vs-account comparison.
//!
//! # Naming-collision warning
//!
//! This crate is deliberately **not** named anything containing "journal":
//! `vyapaar-ledger-journal` already owns the double-entry accounting meaning.
//! This crate's "journal" is a trader's performance journal, following the
//! `vyapaar-ledger-<subdomain>` convention
//! (`vyapaar-ledger-oms`, `vyapaar-ledger-portfolio`, `vyapaar-ledger-journal`,
//! `vyapaar-ledger-allocation`, `vyapaar-ledger-performance`).
//!
//! # Layering
//!
//! This crate is a **thin layer on top of `vyapaar-ledger-portfolio`**, not a
//! re-implementation of FIFO lot matching. `vyapaar-ledger-portfolio` already
//! performs FIFO round-trip derivation via [`AccountState::match_lots`]
//! returning [`MatchedLot`](vyapaar_ledger_portfolio::MatchedLot) — each one a
//! closed trade leg with realized PnL. This crate consumes that derived
//! closed-trade stream, attaches user annotations, and computes analytics. It
//! does not own fills or lot-matching.
//!
//! # Zero-I/O, zero-tokio, zero-async
//!
//! A pure synchronous domain library matching the workspace policy: no network,
//! no filesystem, no clock queries. Every time-dependent input arrives as an
//! explicit timestamp.
//!
//! # Zero floats
//!
//! All PnL, R-multiple, ratio, and edge-score figures are exact
//! [`rust_decimal::Decimal`]. No `f32`/`f64` crosses money/price/quantity paths.
//! (`avg_duration_ms` is a timing figure, not money, and is carried as
//! `Option<u64>`.)
//!
//! # TDD
//!
//! All tests live in `tests/` (parallel test directory rule); no inline
//! `#[cfg(test)]` in `src/`.

// Phase 1 — data model & journal store.
pub mod model;
pub mod store;
// Phase 2 — performance metrics engine + edge-score composite.
pub mod metrics;
// Phase 3 — daily P&L calendar aggregation + account comparison.
pub mod calendar;
pub mod compare;

// Re-export the canonical kernel vocabulary so downstream consumers import one
// place.
pub use vyapaar_core::{Currency, InstrumentId, Money, OrderSide, PositionSide, Price, Quantity};

// Re-export the portfolio types this crate layers on, for convenience.
pub use vyapaar_ledger_portfolio::{
    AccountState, HeldPosition, MatchedLot, PortfolioSnapshot, TaxLot,
};
