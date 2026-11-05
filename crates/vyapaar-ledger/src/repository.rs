//! Pure Domain Repository Traits & DDD Interfaces.
//!
//! Ported from `honba-kokoro/src/repository.rs` (migration guide Fix 5) with
//! the Tier 1B deduplication fixes:
//! - The strongly typed identifiers (`AccountId`, `AlgoId`,
//!   `ClientOrderId`, `TransactionId`) and [`FillReport`] are no longer
//!   duplicated here — they are the canonical [`vyapaar_core`] types.
//! - [`AccountCode`] and the chart of accounts live in
//!   [`vyapaar_ledger_journal`]; this module re-exports it.
//! - [`TaxLot`], [`MatchedLot`] and [`LotFill`] live in
//!   [`vyapaar_ledger_portfolio`]; the matching logic is delegated to the
//!   [`vyapaar_ledger_portfolio::AccountState`] aggregate instead of being
//!   re-implemented in the repository.
//!
//! The in-memory implementations are the reference mocks for pure domain
//! simulation and tests. Traits are object-safe (`Send + Sync`) with pure
//! zero-I/O semantics; no persistence driver (`sqlx` et al.) leaks in here.

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use vyapaar_core::{
    AccountId, AlgoId, ClientOrderId, Currency, FillReport, InstrumentId, Money, OrderSide,
    OrderStatus, OrderType, PositionSide, Price, Quantity, TimeInForce, TransactionId,
};
use vyapaar_ledger_portfolio::{AccountState, LotFill, MatchedLot, TaxLot};

/// Canonical ledger account code in the Chart of Accounts (owned by the
/// journal crate; re-exported here as part of the domain vocabulary).
pub use vyapaar_ledger_journal::AccountCode;

// ——————————————————————————————————————————————————————————————————————————
// Domain Error
// ——————————————————————————————————————————————————————————————————————————

/// Comprehensive domain-layer error enumeration.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DomainError {
    #[error("Entity not found: {0}")]
    NotFound(String),
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("Unbalanced ledger: debits total {0} != credits total {1}")]
    UnbalancedLedger(Decimal, Decimal),
    #[error("Insufficient balance: {0}")]
    InsufficientBalance(String),
    #[error("Illegal state transition: {0}")]
    IllegalStateTransition(String),
    #[error("Concurrency collision: {0}")]
    ConcurrencyConflict(String),
    #[error("Storage failure: {0}")]
    Storage(String),
}

// ——————————————————————————————————————————————————————————————————————————
// Domain Aggregates & Records
// ——————————————————————————————————————————————————————————————————————————

/// Canonical order record stored and queried via [`OrderRepository`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderRecord {
    pub client_order_id: ClientOrderId,
    pub broker_order_id: Option<String>,
    pub algo_id: Option<AlgoId>,
    pub account_id: AccountId,
    pub instrument: InstrumentId,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub time_in_force: TimeInForce,
    pub price: Option<Price>,
    pub quantity: Quantity,
    pub filled_quantity: Quantity,
    pub avg_fill_price: Option<Price>,
    pub status: OrderStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl OrderRecord {
    pub fn is_active(&self) -> bool {
        matches!(
            self.status,
            OrderStatus::PendingNew | OrderStatus::New | OrderStatus::PartiallyFilled
        )
    }
}

/// Canonical account record stored and queried via [`AccountRepository`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountRecord {
    pub account_id: AccountId,
    pub algo_id: Option<AlgoId>,
    pub cash: Money,
    pub status: String,
    pub updated_at: DateTime<Utc>,
}

/// Aggregate position record representing an account's net exposure in an
/// instrument (the repository's read-side projection of the underlying
/// [`AccountState`] aggregate).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PositionRecord {
    pub account_id: AccountId,
    pub instrument: InstrumentId,
    pub side: PositionSide,
    pub quantity: Quantity,
    pub avg_price: Price,
    pub realized_pnl: Money,
    pub unrealized_pnl: Money,
    pub lots: Vec<TaxLot>,
    pub updated_at: DateTime<Utc>,
}

/// Double-entry posting side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PostingSide {
    Debit,
    Credit,
}

/// Double-entry posting entry (debit or credit leg).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostingEntry {
    pub account: AccountCode,
    pub amount: Money,
}

/// A balanced double-entry financial journal transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalTransaction {
    pub id: TransactionId,
    pub timestamp: DateTime<Utc>,
    pub description: String,
    pub debits: Vec<PostingEntry>,
    pub credits: Vec<PostingEntry>,
}

impl JournalTransaction {
    pub fn new(
        id: impl Into<TransactionId>,
        timestamp: DateTime<Utc>,
        description: impl Into<String>,
        debits: Vec<PostingEntry>,
        credits: Vec<PostingEntry>,
    ) -> Result<Self, DomainError> {
        let tx = Self {
            id: id.into(),
            timestamp,
            description: description.into(),
            debits,
            credits,
        };
        tx.validate()?;
        Ok(tx)
    }

    pub fn validate(&self) -> Result<(), DomainError> {
        if self.debits.is_empty() || self.credits.is_empty() {
            return Err(DomainError::Validation(
                "Transaction must contain at least one debit and one credit leg".to_string(),
            ));
        }

        let mut total_debit = Decimal::ZERO;
        for leg in &self.debits {
            if leg.amount.amount <= Decimal::ZERO {
                return Err(DomainError::Validation(
                    "Debit amount must be strictly positive".to_string(),
                ));
            }
            total_debit += leg.amount.amount;
        }

        let mut total_credit = Decimal::ZERO;
        for leg in &self.credits {
            if leg.amount.amount <= Decimal::ZERO {
                return Err(DomainError::Validation(
                    "Credit amount must be strictly positive".to_string(),
                ));
            }
            total_credit += leg.amount.amount;
        }

        if total_debit != total_credit {
            return Err(DomainError::UnbalancedLedger(total_debit, total_credit));
        }

        Ok(())
    }
}

// ——————————————————————————————————————————————————————————————————————————
// Pure Domain Repository Traits (Dependency Inversion)
// ——————————————————————————————————————————————————————————————————————————

/// Pure domain repository managing order lifecycle and state retrieval.
pub trait OrderRepository: Send + Sync {
    /// Retrieve an order by its canonical client order ID.
    fn get_order(&self, id: &ClientOrderId) -> Result<Option<OrderRecord>, DomainError>;

    /// Save or update an order record idempotently.
    fn save_order(&mut self, record: &OrderRecord) -> Result<(), DomainError>;

    /// List all currently active (unfilled / partially filled) orders for a given algo ID.
    fn list_active_orders(&self, algo_id: &AlgoId) -> Result<Vec<OrderRecord>, DomainError>;
}

/// Pure domain repository managing trading accounts.
pub trait AccountRepository: Send + Sync {
    /// Retrieve an account by its canonical identifier.
    fn get_account(&self, id: &AccountId) -> Result<Option<AccountRecord>, DomainError>;

    /// Save or update an account record idempotently.
    fn save_account(&mut self, account: &AccountRecord) -> Result<(), DomainError>;

    /// List all registered trading accounts.
    fn list_accounts(&self) -> Result<Vec<AccountRecord>, DomainError>;
}

/// Pure domain repository managing multi-account FIFO tax-lot positions and
/// matching, delegating to the portfolio aggregate.
pub trait PositionRepository: Send + Sync {
    /// Retrieve all open positions for a given trading account.
    fn get_positions(&self, account_id: &AccountId) -> Result<Vec<PositionRecord>, DomainError>;

    /// Save a new cost-basis lot for an account.
    fn save_lot(&mut self, account_id: &AccountId, lot: &TaxLot) -> Result<(), DomainError>;

    /// Match an incoming fill report against open tax lots using strict FIFO
    /// matching, returning the per-lot realized details.
    ///
    /// The account ID is read from the fill report's `account_id` field.
    fn match_lots(&mut self, fill: &FillReport) -> Result<Vec<MatchedLot>, DomainError>;
}

/// Pure domain repository managing immutable double-entry ledger transactions and balances.
pub trait LedgerRepository: Send + Sync {
    /// Post a balanced journal transaction to the general ledger.
    fn post_entry(&mut self, tx: &JournalTransaction) -> Result<TransactionId, DomainError>;

    /// Retrieve the current instantaneous balance of a specific ledger account code.
    fn get_balance(&self, account: &AccountCode) -> Result<Money, DomainError>;
}

// ——————————————————————————————————————————————————————————————————————————
// In-Memory Reference / Mock Implementations
// ——————————————————————————————————————————————————————————————————————————

/// Thread-safe in-memory order repository for pure domain simulation and tests.
#[derive(Debug, Clone, Default)]
pub struct InMemoryOrderRepository {
    orders: HashMap<ClientOrderId, OrderRecord>,
}

impl InMemoryOrderRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl OrderRepository for InMemoryOrderRepository {
    fn get_order(&self, id: &ClientOrderId) -> Result<Option<OrderRecord>, DomainError> {
        Ok(self.orders.get(id).cloned())
    }

    fn save_order(&mut self, record: &OrderRecord) -> Result<(), DomainError> {
        self.orders
            .insert(record.client_order_id.clone(), record.clone());
        Ok(())
    }

    fn list_active_orders(&self, algo_id: &AlgoId) -> Result<Vec<OrderRecord>, DomainError> {
        let mut active = Vec::new();
        for order in self.orders.values() {
            if order.is_active() && order.algo_id.as_ref() == Some(algo_id) {
                active.push(order.clone());
            }
        }
        active.sort_by_key(|a| a.created_at);
        Ok(active)
    }
}

/// In-memory account repository for simulation and tests.
#[derive(Debug, Clone, Default)]
pub struct InMemoryAccountRepository {
    accounts: HashMap<AccountId, AccountRecord>,
}

impl InMemoryAccountRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl AccountRepository for InMemoryAccountRepository {
    fn get_account(&self, id: &AccountId) -> Result<Option<AccountRecord>, DomainError> {
        Ok(self.accounts.get(id).cloned())
    }

    fn save_account(&mut self, account: &AccountRecord) -> Result<(), DomainError> {
        self.accounts
            .insert(account.account_id.clone(), account.clone());
        Ok(())
    }

    fn list_accounts(&self) -> Result<Vec<AccountRecord>, DomainError> {
        let mut list: Vec<_> = self.accounts.values().cloned().collect();
        list.sort_by(|a, b| a.account_id.cmp(&b.account_id));
        Ok(list)
    }
}

/// In-memory position repository backed by the portfolio [`AccountState`]
/// aggregate: one state per trading account, with the FIFO tax-lot matching
/// delegated to the aggregate (one implementation, three callers).
#[derive(Debug, Clone, Default)]
pub struct InMemoryPositionRepository {
    accounts: HashMap<AccountId, AccountState>,
}

impl InMemoryPositionRepository {
    pub fn new() -> Self {
        Self::default()
    }

    /// Access the underlying [`AccountState`] for an account, creating an
    /// empty INR-denominated state when unseen.
    fn state_for(&mut self, account_id: &AccountId) -> &mut AccountState {
        self.accounts
            .entry(account_id.clone())
            .or_insert_with(|| AccountState::new(Currency::Inr, vec![]))
    }

    fn empty_state_for(&self, account_id: &AccountId) -> AccountState {
        self.accounts
            .get(account_id)
            .cloned()
            .unwrap_or_else(|| AccountState::new(Currency::Inr, vec![]))
    }

    /// Project the read-side [`PositionRecord`]s of an account from its
    /// underlying aggregate: one record per held position, carrying only the
    /// aggregate's *open* (non-exhausted) lots.
    fn project(&self, account_id: &AccountId, state: &AccountState) -> Vec<PositionRecord> {
        state
            .positions
            .iter()
            .map(|pos| PositionRecord {
                account_id: account_id.clone(),
                instrument: pos.instrument.clone(),
                side: pos.side,
                quantity: pos.quantity,
                avg_price: pos.entry_price,
                realized_pnl: pos.realized_pnl,
                unrealized_pnl: Money::inr(Decimal::ZERO),
                lots: state
                    .lots
                    .iter()
                    .filter(|l| l.instrument == pos.instrument && !l.is_exhausted())
                    .cloned()
                    .collect(),
                updated_at: pos.updated_at,
            })
            .collect()
    }
}

impl PositionRepository for InMemoryPositionRepository {
    fn get_positions(&self, account_id: &AccountId) -> Result<Vec<PositionRecord>, DomainError> {
        let state = self.empty_state_for(account_id);
        Ok(self.project(account_id, &state))
    }

    fn save_lot(&mut self, account_id: &AccountId, lot: &TaxLot) -> Result<(), DomainError> {
        self.state_for(account_id).record_lot(lot);
        Ok(())
    }

    fn match_lots(&mut self, fill: &FillReport) -> Result<Vec<MatchedLot>, DomainError> {
        // Get account_id from fill report
        let account_id =
            fill.account_id
                .as_deref()
                .map(AccountId::new)
                .ok_or(DomainError::Validation(
                    "FillReport missing account_id - cannot match lots".to_string(),
                ))?;
        // The aggregate owns the matching rule; the repository only adapts the
        // canonical core FillReport into the portfolio matching input.
        let lot_fill = LotFill {
            instrument: fill.instrument.clone(),
            side: fill.side,
            price: fill.price,
            quantity: fill.quantity,
            timestamp: fill.timestamp,
        };
        Ok(self.state_for(&account_id).match_lots(&lot_fill))
    }
}

/// In-memory general ledger repository tracking double-entry transactions and balances.
#[derive(Debug, Clone, Default)]
pub struct InMemoryLedgerRepository {
    transactions: Vec<JournalTransaction>,
    balances: BTreeMap<AccountCode, Decimal>,
    base_currency: Currency,
}

impl InMemoryLedgerRepository {
    pub fn new(base_currency: Currency) -> Self {
        Self {
            transactions: Vec::new(),
            balances: BTreeMap::new(),
            base_currency,
        }
    }
}

impl LedgerRepository for InMemoryLedgerRepository {
    fn post_entry(&mut self, tx: &JournalTransaction) -> Result<TransactionId, DomainError> {
        tx.validate()?;

        for deb in &tx.debits {
            *self.balances.entry(deb.account.clone()).or_default() += deb.amount.amount;
        }
        for cred in &tx.credits {
            *self.balances.entry(cred.account.clone()).or_default() -= cred.amount.amount;
        }

        self.transactions.push(tx.clone());
        Ok(tx.id.clone())
    }

    fn get_balance(&self, account: &AccountCode) -> Result<Money, DomainError> {
        let bal = self.balances.get(account).copied().unwrap_or(Decimal::ZERO);
        Ok(Money {
            amount: bal,
            currency: self.base_currency,
        })
    }
}
