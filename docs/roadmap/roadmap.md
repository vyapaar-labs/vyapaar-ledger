# `vyapaar-ledger` — TDD & Engineering Roadmap

## Milestone 1: `vyapaar-ledger-oms` Refactor & Migration (P0)
- [x] Port `OrderStateMachine` and order transition rules into `crates/vyapaar-ledger-oms`.
- [x] Replace duplicate local types (`Exchange`, `Price`, `Quantity(u64)`, `OrderSide::{Long,Short}`) with `vyapaar_core` canonical types.
- [x] Fix `OrderStateMachine::register_order` to record `target_quantity: Quantity` so `process_fill` accurately distinguishes `PartiallyFilled` vs `Filled`.
- [x] Remove `async fn submit_order`, `async fn cancel_order`, and `Utc::now()` calls.
- [x] Extract all inline `#[cfg(test)]` tests to `crates/vyapaar-ledger-oms/tests/`.

## Milestone 2: `vyapaar-ledger-portfolio` Refactor & Migration (P0)
- [x] Port `ClearingCalendar`, `DematHoldings`, `SettlementEngine`, `CorporateAction`, `AccountState`, `marks.rs`, `engine.rs`, `state.rs` into `crates/vyapaar-ledger-portfolio`.
- [x] Extract `src/tests/mod.rs` to `crates/vyapaar-ledger-portfolio/tests/`.

## Milestone 3: `vyapaar-ledger-journal` Refactor & Migration (P0)
- [x] Port `DoubleEntryLedger`, `Transaction`, `TaxCalculator`, `TaxBreakdown` from `honba-kokoro-ledger` into `crates/vyapaar-ledger-journal`.
- [x] Extract inline `#[cfg(test)]` tests and `src/golden_tests/` to `crates/vyapaar-ledger-journal/tests/`.

## Milestone 4: `vyapaar-ledger` Facade & Repository Traits (P0)
- [x] Port `repository.rs` and `sizing.rs` from `honba-kokoro/crates/honba-kokoro` into `crates/vyapaar-ledger`.
- [x] Re-export `ClientOrderId`, `AlgoId`, `AccountId`, `TransactionId` from `vyapaar-core`.
