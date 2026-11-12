# Gap 0005 — Performance `MetricsInput::new()` Mints `as_of` via `Utc::now()`

**Status**: open — tracked by [plan 0004](../plans/0004-performance-journal-edge-and-determinism.md)

## What's Stubbed / Violated

`crates/vyapaar-ledger-performance/src/metrics.rs`:

```rust
impl MetricsInput {
    pub fn new(closed_trades: Vec<ClosedTrade>) -> Self {
        Self {
            closed_trades,
            open_trades: 0,
            equity_curve: Vec::new(),
            as_of: Utc::now(), // test-only placeholder; callers should set explicitly
        }
    }
```

This is the **only** `Utc::now()` call in `crates/*/src/` across the whole
workspace, and it violates the temporal-determinism invariant (no wall-clock
queries in domain code): two calls to `MetricsInput::new` in the same process
stamp different `as_of` values, so identical inputs are not byte-identical.

## Why

- Replaying the same closed-trade stream must produce byte-identical metrics in
  backtests and live (ADR 0032).
- `compare_accounts` / `compare_account_vs_prior` already inject `as_of`
  explicitly, so the convenience constructor is the only offender.

## How to Close It

1. Remove the `Utc::now()` default from `MetricsInput::new`.
2. Make `as_of` a required constructor argument (`MetricsInput::new(trades, as_of)`)
   or keep a `Default` that uses a deterministic epoch.
3. Update the call sites in `tests/` that rely on the default.
4. Add a compile-time/grep guard so no `Utc::now()` re-enters `crates/*/src/`.

## Verification

`grep -rn "Utc::now" crates/*/src/` returns nothing; the `now_is_deterministic`
tests in `tests/metrics_tests.rs` / `tests/store_tests.rs` still pass.
