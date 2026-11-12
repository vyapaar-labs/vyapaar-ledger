# Gap 0004 — Discrete / Whole-Share Allocation Not Implemented

**Status**: open — tracked by [plan 0003](../plans/0003-allocation-phase-2-4.md)

## What's Missing

`crates/vyapaar-ledger-allocation` produces continuous fractional `TargetWeights`
summing to 1. There is no discrete / whole-share allocation step that maps the
continuous target weights onto an actual tradable book (whole shares × an
execution currency, respecting lot sizes), which is the recorded Phase 4 scope
of plan 0001 (`docs/plans/README.md`).

## Why

- A live trading caller (eventually `vyapaar-exec`) cannot place fractional
  shares; without a deterministic rounding step, research weights are not
  directly executable.
- The workspace zero-floats and research-live parity invariants require the
  discrete step to be pure `Decimal` and deterministic.

## How to Close It

1. Add a discrete-allocation entry point (e.g. `allocate_discrete`) that takes
  the continuous `TargetWeights`, a portfolio equity `Money`, and per-instrument
  lot sizes and returns whole-share `Quantity`s.
2. Choose a deterministic rounding scheme (largest-remainder / Hamilton method,
  or iterative lot rounding) in `Decimal`.
3. Document and test the residual-cash / rounding-error handling.

## Verification

`cargo test --workspace --profile prod` green with discrete-allocation tests;
whole-share output sums within one lot of the executable equity.
