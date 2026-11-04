# `vyapaar-ledger` — TDD & Engineering Roadmap

## Milestone 1: `vyapaar-ledger-oms` Refactor & Migration (P0)
- [ ] Port `OrderStateMachine` and order transition rules into `crates/vyapaar-ledger-oms`.
- [ ] Replace duplicate local types (`Exchange`, `Price`, `Quantity(u64)`, `OrderSide::{Long,Short}`) with `vyapaar_core` canonical types.
- [ ] Fix `OrderStateMachine::register_order` to record `target_quantity: Quantity` so `process_fill` accurately distinguishes `PartiallyFilled` vs `Filled`.
- [ ] Remove `async fn submit_order`, `async fn cancel_order`, and `Utc::now()` calls.
- [ ] Extract all inline `#[cfg(test)]` tests to `crates/vyapaar-ledger-oms/tests/`.

## Milestone 2: `vyapaar-ledger-portfolio` Refactor & Migration (P0)
- [ ] Port `ClearingCalendar`, `DematHoldings`, `SettlementEngine`, `CorporateAction`, `AccountState`, `marks.rs`, `engine.rs`, `state.rs` into `crates/vyapaar-ledger-portfolio`.
- [ ] Extract `src/tests/mod.rs` to `crates/vyapaar-ledger-portfolio/tests/`.

## Milestone 3: `vyapaar-ledger-journal` Refactor & Migration (P0)
- [ ] Port `DoubleEntryLedger`, `Transaction`, `TaxCalculator`, `TaxBreakdown` from `honba-kokoro-ledger` into `crates/vyapaar-ledger-journal`.
- [ ] Extract inline `#[cfg(test)]` tests and `src/golden_tests/` to `crates/vyapaar-ledger-journal/tests/`.

## Milestone 4: `vyapaar-ledger` Facade & Repository Traits (P0)
- [ ] Port `repository.rs` and `sizing.rs` from `honba-kokoro/crates/honba-kokoro` into `crates/vyapaar-ledger`.
- [ ] Re-export `ClientOrderId`, `AlgoId`, `AccountId`, `TransactionId` from `vyapaar-core`.
