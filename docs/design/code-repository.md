# `vyapaar-ledger` — Repository Traits Reference

Documents `crates/vyapaar-ledger/src/repository.rs`: pure domain repository traits and
in-memory reference implementations (dependency inversion — domain owns traits,
infrastructure implements them).

## Repository Traits

| Trait | Methods | Purpose |
|-------|---------|---------|
| `OrderRepository` | `get_order`, `save_order`, `list_active_orders` | Order lifecycle state |
| `AccountRepository` | `get_account`, `save_account`, `list_accounts` | Trading accounts |
| `PositionRepository` | `get_positions`, `save_lot`, `match_lots` | FIFO tax-lot positions (delegates to `AccountState`) |
| `LedgerRepository` | `post_entry`, `get_balance` | Double-entry journal transactions |

All traits are `Send + Sync` (object-safe) for `Arc<dyn Trait>` injection.

## Domain Error (`DomainError`)

| Variant | Meaning |
|---------|---------|
| `NotFound(String)` | Entity not found |
| `Validation(String)` | Validation failure |
| `UnbalancedLedger(Decimal, Decimal)` | Debits != credits |
| `InsufficientBalance(String)` | Cash insufficient |
| `IllegalStateTransition(String)` | Invalid state change |
| `ConcurrencyConflict(String)` | Optimistic locking collision |
| `Storage(String)` | Storage failure |

## Domain Records

| Record | Fields |
|--------|--------|
| `OrderRecord` | client_order_id, broker_order_id, algo_id, account_id, instrument, side, order_type, time_in_force, price, quantity, filled_quantity, avg_fill_price, status, timestamps |
| `AccountRecord` | account_id, algo_id, cash (`Money`), status, updated_at |
| `PositionRecord` | account_id, instrument, side, quantity, avg_price, realized_pnl, unrealized_pnl, lots, updated_at |
| `JournalTransaction` | id (`TransactionId`), timestamp, description, debits, credits |

## In-Memory Implementations

- `InMemoryOrderRepository` — `HashMap<ClientOrderId, OrderRecord>`
- `InMemoryAccountRepository` — `HashMap<AccountId, AccountRecord>`
- `InMemoryPositionRepository` — `HashMap<AccountId, AccountState>` (delegates matching to aggregate)
- `InMemoryLedgerRepository` — `Vec<JournalTransaction>` + `BTreeMap<AccountCode, Decimal>` balances

## Zero-I/O Purity

Traits declare no async, no filesystem, no network. Implementations are pure in-memory
reference mocks for simulation and tests. No `sqlx`, `tokio::fs`, or database drivers.

## Related

- `vyapaar-ledger-oms` — `OrderStateMachine` produces orders saved via `OrderRepository`.
- `vyapaar-ledger-portfolio` — `AccountState` owns FIFO matching used by `PositionRepository`.
- `vyapaar-ledger-journal` — `DoubleEntryLedger` produces transactions saved via `LedgerRepository`.
