# `vyapaar-ledger` — Repository Traits Reference

Documents `crates/vyapaar-ledger/src/repository.rs`: pure domain repository
traits and in-memory reference implementations (dependency inversion — the
domain owns the traits, infrastructure implements them).

## Repository Traits

All traits are `Send + Sync` (object-safe) for `Arc<dyn Trait>` injection:

| Trait | Methods | Purpose |
|-------|---------|---------|
| `OrderRepository` | `get_order`, `save_order`, `list_active_orders(algo_id)` | Order lifecycle state; active = `PendingNew` / `New` / `PartiallyFilled` (`OrderRecord::is_active`). |
| `AccountRepository` | `get_account`, `save_account`, `list_accounts` | Trading accounts. |
| `PositionRepository` | `get_positions`, `save_lot`, `match_lots(fill)` | FIFO tax-lot positions — reads the account ID from the `FillReport.account_id`, delegates matching to the `AccountState` aggregate. |
| `LedgerRepository` | `post_entry`, `get_balance` | Double-entry journal transactions and balances. |

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
| `OrderRecord` | client_order_id, broker_order_id, algo_id, account_id, instrument, side, order_type, time_in_force, price, quantity, filled_quantity, avg_fill_price, status, created_at, updated_at |
| `AccountRecord` | account_id, algo_id, cash (`Money`), status, updated_at |
| `PositionRecord` | account_id, instrument, side (`PositionSide`), quantity, avg_price, realized_pnl, unrealized_pnl, lots (open `TaxLot`s), updated_at |
| `JournalTransaction` | id (`TransactionId`), timestamp, description, debits, credits (`PostingEntry`) |
| `PostingEntry` / `PostingSide` | Single debit/credit leg: `account` (`AccountCode`), `amount` (`Money`); `Debit` / `Credit`. |

`JournalTransaction::new(...)`/`validate()` enforce the double-entry invariant:
non-empty legs, strictly positive amounts, `sum(debits) == sum(credits)`.

## In-Memory Implementations

- `InMemoryOrderRepository` — `HashMap<ClientOrderId, OrderRecord>`;
  `list_active_orders` sorts by `created_at`.
- `InMemoryAccountRepository` — `HashMap<AccountId, AccountRecord>`;
  `list_accounts` sorts by `account_id`.
- `InMemoryPositionRepository` — `HashMap<AccountId, AccountState>`: one state
  per trading account; `save_lot` calls `AccountState::record_lot`,
  `match_lots` adapts the canonical `FillReport` into a `LotFill` and delegates
  to `AccountState::match_lots`; `get_positions` projects read-side
  `PositionRecord`s from the aggregate (INR-denominated empty state for unseen
  accounts).
- `InMemoryLedgerRepository` — `Vec<JournalTransaction>` +
  `BTreeMap<AccountCode, Decimal>` balances; `post_entry` validates and
  applies debits (`+=`) and credits (`-=`), returns the transaction ID.

## Zero-I/O Purity

Traits declare no async, no filesystem, no network. Implementations are pure
in-memory reference mocks for simulation and tests. No `sqlx`, `tokio::fs`, or
database drivers.

## Related

- `crates/vyapaar-ledger/src/lib.rs` — facade re-exports (repository types,
  canonical `AccountId`/`AlgoId`/`ClientOrderId`/`FillReport`/`TransactionId`
  from `vyapaar_core`, `LedgerTransaction`, `DoubleEntryLedger`,
  `LedgerError`, `MatchedLot`, `TaxLot`).
- `vyapaar-ledger-oms` — `OrderStateMachine` produces orders saved via
  `OrderRepository`.
- `vyapaar-ledger-portfolio` — `AccountState` owns FIFO matching used by
  `PositionRepository`.
- `vyapaar-ledger-journal` — `DoubleEntryLedger` produces transactions saved via
  `LedgerRepository`; `AccountCode` is owned by the journal.
