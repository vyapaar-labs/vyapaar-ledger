# `vyapaar-ledger` — DDD Architecture & Pure Domain Heart

## 1. Bounded Context & Responsibility
`vyapaar-ledger` (Tier 1B, ported and refactored from `honba-kokoro`) is the **Zero-I/O Domain Heart** managing the post-risk order lifecycle, FIFO tax-lot & T+1 Demat settlement accounting, and double-entry bookkeeping.

## 2. Sub-Crate Topology

| Crate | Aggregate Root / Domain Service | Responsibility |
| :--- | :--- | :--- |
| `vyapaar-ledger-oms` | `OrderStateMachine` / `OrderBook` | Deterministic SEBI order lifecycle FSM (`PendingNew` → `New` → `PartiallyFilled` → `Filled` / `Cancelled` / `Rejected` / `Expired`), `ClientOrderId` idempotency, target vs. cumulative fill quantity tracking. |
| `vyapaar-ledger-portfolio` | `Portfolio` / `AccountState`, `DematHoldings`, `SettlementEngine` | FIFO tax-lot matching (`TaxLot`, `MatchedLot`), realized & unrealized MTM PnL, Indian corporate actions (`Split`, `Bonus`, `Dividend`, `Demerger`), and ICCL T+1 Demat rolling settlement. |
| `vyapaar-ledger-journal` | `DoubleEntryLedger`, `TaxCalculator` | Immutable double-entry general ledger (`sum(Debits) == sum(Credits)`), Indian chart of accounts (`AccountCode`), and statutory charge calculation. |
| `vyapaar-ledger` | Repository Traits & Facade | Re-exports `oms`, `portfolio`, `journal`, plus pure domain repository traits (`OrderRepository`, `AccountRepository`, `PositionRepository`, `LedgerRepository`) and in-memory implementations. |

## 3. Critical DDD Refactorings Applied in `vyapaar-ledger`
1. **100% Unified on `vyapaar-core` Primitives**: `vyapaar-ledger-oms` and `vyapaar-ledger-journal` no longer define duplicate `Exchange`, `InstrumentId`, `Price`, `Quantity(u64)`, `Money`, or `OrderSide::{Long,Short}`.
2. **Pure Zero-I/O**: `tokio` and `async fn` are completely removed from `vyapaar-ledger`. All functions are synchronous and accept explicit `DateTime<Utc>`.
3. **Aggregate-Owned Domain Logic**: FIFO tax-lot matching (`match_lots`) belongs to the portfolio domain aggregate and is invoked by `InMemoryPositionRepository`, rather than trapping business logic inside the repository implementation.
