//! # व्यापार・खातावही وأदार vyapaar-ledger
//!
//! Pure Domain Heart of the Vyapaar quantitative trading ecosystem: the
//! domain contract facade. Subsystems: order management system
//! ([`oms`]), FIFO tax-lot portfolio engine ([`portfolio`]), double-entry
//! general ledger ([`journal`]), pure domain repository traits
//! ([`repository`]), and the dynamic position sizing service hook
//! ([`sizing`]).
//!
//! Architectural invariants (Tier 1B, ADR 0032):
//! - Pure zero-I/O: no sockets, no filesystem I/O, no database drivers.
//! - Zero floats: exact 128-bit decimal arithmetic (`rust_decimal::Decimal`).
//! - Temporal determinism: all calculations receive explicit `DateTime<Utc>`;
//!   the domain never mints a timestamp or a UUID.
//! - Strict Dependency Inversion: the domain layer owns repository traits;
//!   infrastructure implements them.
//!
//! # Migration notes (Tier 1B, `honba-kokoro` → `vyapaar-ledger`)
//!
//! - The former local duplicate identifiers (`AccountCode`, `AccountId`,
//!   `AlgoId`, `ClientOrderId`, `TransactionId`, `FillReport`, `TaxLot`,
//!   `MatchedLot`) were deleted: ids and fill reports come from the shared
//!   kernel [`vyapaar_core`], chart-of-accounts codes come from the
//!   [`journal`], tax-lot vocabulary comes from [`portfolio`]. This crate
//!   now owns only its genuine projections (`OrderRecord`, `AccountRecord`,
//!   `PositionRecord`, posting shapes) and the in-memory reference
//!   repositories.

pub use vyapaar_ledger_journal as journal;
pub use vyapaar_ledger_journal as ledger;
pub use vyapaar_ledger_oms as oms;
pub use vyapaar_ledger_portfolio as portfolio;

pub mod repository;
pub mod sizing;

pub use repository::{
    AccountCode, AccountRecord, AccountRepository, DomainError, InMemoryAccountRepository,
    InMemoryLedgerRepository, InMemoryOrderRepository, InMemoryPositionRepository,
    JournalTransaction, LedgerRepository, OrderRecord, OrderRepository, PositionRecord,
    PositionRepository, PostingEntry, PostingSide,
};
pub use sizing::{Portfolio, evaluate_risk_budget};

// Canonical vocabulary, re-exported so downstream gates import only this crate.
pub use vyapaar_core::{AccountId, AlgoId, ClientOrderId, FillReport, TransactionId};
pub use vyapaar_ledger_journal::Transaction as LedgerTransaction;
pub use vyapaar_ledger_journal::{DoubleEntryLedger, LedgerError};
pub use vyapaar_ledger_portfolio::{MatchedLot, TaxLot};
