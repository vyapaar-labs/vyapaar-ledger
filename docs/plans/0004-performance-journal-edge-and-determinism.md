# Plan 0004 — Performance Journal: Temporal Determinism & Real Edge Score

**Status**: Ready for Implementation

**Scope**: `vyapaar-ledger-performance`

**Gaps covered**:
- [0005](../gaps/0005-performance-metrics-clock-query.md) — `MetricsInput::new()` mints `as_of: Utc::now()`
- [0006](../gaps/0006-performance-edge-score-v1-placeholder.md) — v1 placeholder edge-score formula

## 0. Executive Summary & Objective

Plan 0002 (commits `2a3a04e`, `1b7fb97`) shipped the performance journal
(data model, metrics engine, edge score, calendar, comparison). Two v1 items
must be hardened before the crate is production-trustworthy: (a) the only
wall-clock query in the entire workspace's `crates/*/src/`, and (b) the
documented placeholder edge-score formula.

## 1. Context & Gaps Covered

`MetricsInput::new()` defaults `as_of` to `Utc::now()` (gap 0005) — a
temporal-determinism violation. The `EdgeScore` composite uses a transparent
but arbitrary equal-weight formula and an explicit v1 schema version (gap
0006). `compare.rs` already injects `as_of`, so the fix surface is small.

## 2. Component Specifications & Phases

### Phase 1 — Remove the clock query (gap 0005)

- Make `as_of` a required constructor argument
  (`MetricsInput::new(closed_trades, as_of)`) or a deterministic-epoch
  `Default`; remove `Utc::now()` from `src/`.
- Update every test call site and add a regression assertion that identical
  inputs give identical metrics regardless of wall-clock time.

### Phase 2 — Define and implement the real edge score (gap 0006)

- Write the component scoring brackets and blend weights as a spec (either
  research-backed or an explicit decision to keep equal weights, documented in
  the module).
- Bump `EDGE_SCORE_VERSION` to 2, implement, and pin golden vectors for a known
  trade set (win rate, profit factor, avg win/loss, drawdown, recovery,
  consistency).

## 3. Acceptance Criteria

- `grep -rn "Utc::now" crates/*/src/` returns nothing.
- `cargo test --workspace --profile prod` green, including the new golden
  edge-score vectors; `now_is_deterministic` tests pass.
- `cargo fmt --check` and `cargo clippy --workspace --all-targets --profile prod -- -D warnings` clean.
