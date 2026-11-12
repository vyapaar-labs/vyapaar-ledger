# Gap 0003 — Allocation Bound Handling Is Clip-and-Renormalize, Not Exact Bound-Constrained QP

**Status**: open — tracked by [plan 0003](../plans/0003-allocation-phase-2-4.md)

## What's Stubbed / Simplified

`crates/vyapaar-ledger-allocation/src/lib.rs` documents a Phase 1
simplification: `optimize()` solves the *unconstrained* optimum in exact
`Decimal` arithmetic, then projects it into the box via
`apply_bounds` — iterative clip-and-renormalize (clip to `[min, max]`,
re-scale strictly in-bound weights so the portfolio re-sums to 1, repeat until
stable). An exact bound-constrained quadratic-programming solve is explicitly
deferred ("is a follow-on refinement, not part of this slice").

## Why

- Clip-and-renormalize is a heuristic: it does not return the true
  constrained-optimum portfolio and can leave the solution further from the
  optimum than an exact QP for correlated universes.
- The unconstrained `Σ·w = μ − r_f` solve already exists; the box-constrained
  Kuhn–Tucker solution is the natural extension.

## How to Close It

1. Implement an active-set (or projected-gradient) box-constrained solver
  entirely in `Decimal` (no `f64`).
2. Replace `apply_bounds` in the `MeanVariance` and `RiskParity` paths with the
  exact solver; keep the feasibility checks (`min > max`,
  `sum(min) > 1`, `sum(max) < 1`).
3. Add golden-vector tests comparing the exact solution against the
  clip-and-renormalize output and against a reference solver on the same box.

## Verification

`cargo test --workspace --profile prod` green; `apply_bounds` is either removed
or reduced to a pure feasibility prover.
