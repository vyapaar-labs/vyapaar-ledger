# Plan 0003 — Allocation Engine: Phases 2–4 (Hrp, Black-Litterman, Exact Bounds, Discrete Allocation)

**Status**: Ready for Implementation

**Scope**: `vyapaar-ledger-allocation`

**Gaps covered**:
- [0001](../gaps/0001-allocation-hrp-not-implemented.md) — `Hrp` objective
- [0002](../gaps/0002-allocation-black-litterman-not-implemented.md) — `BlackLitterman` objective
- [0003](../gaps/0003-allocation-exact-bound-constrained-qp.md) — exact bound-constrained QP
- [0004](../gaps/0004-allocation-discrete-whole-share.md) — discrete / whole-share allocation

## 0. Executive Summary & Objective

Phase 1 (`docs/plans/README.md` plan 0001, commit `b35a314`) shipped
`AllocationObjective::{MeanVariance, RiskParity}` with box bounds in pure
`Decimal`. This plan completes the declared-but-`NotImplemented` objectives
(`Hrp`, `BlackLitterman`), replaces the clip-and-renormalize bound projection
with an exact bound-constrained solve, and adds the discrete / whole-share step
that makes target weights executable. All numerics remain exact
`rust_decimal::Decimal`; `as_of` stays caller-injected.

## 1. Context & Gaps Covered

The enum variants already exist for wire stability, but `optimize()` returns
`AllocationError::NotImplemented` for them (gaps 0001, 0002), the bound
projection is a documented heuristic (gap 0003), and no discrete rounding
exists (gap 0004).

## 2. Component Specifications & Phases

### Phase 2 — Hrp (gap 0001)

- Correlation distance from the covariance matrix; single-linkage
  dendrogram; quasi-diagonalization + recursive bisection risk allocation.
- All clustering math in `Decimal` (no `f64`); document the fixed-point
  tolerance.
- Route `AllocationObjective::Hrp` in `optimize()`; golden vectors pinned in
  `tests/optimize_tests.rs`.

### Phase 3 — Black-Litterman (gap 0002)

- Reverse-optimize market-implied returns from a benchmark weight vector:
  `τ·Σ·w_mkt`.
- View-posterior blend: pick matrix `P`, view covariance `Ω`,
  `( (τΣ)⁻¹ + PᵀΩ⁻¹P )⁻¹ · ( (τΣ)⁻¹·π + PᵀΩ⁻¹·q )`.
- New input type (or extension of `PortfolioState`) carrying views; route the
  objective; golden vectors.

### Phase 3b — Exact bound-constrained QP (gap 0003)

- Active-set / projected-gradient box-constrained solve in `Decimal`,
  replacing `apply_bounds` in both implemented objectives.
- Keep feasibility checks; add exact-vs-heuristic golden comparisons.

### Phase 4 — Discrete / whole-share allocation (gap 0004)

- `allocate_discrete(target_weights, equity, lot_sizes)` → whole-share
  `Quantity`s via a deterministic largest-remainder / iterative rounding
  scheme; document residual-cash handling.

## 3. Acceptance Criteria

- `cargo test --workspace --profile prod` green; no `NotImplemented` path
  remains for `Hrp` / `BlackLitterman`.
- Weights always sum to 1 within `1e-9`; bounds exactly satisfied for the
  golden universes.
- Discrete allocation is deterministic and executable within one lot of the
  target equity.
- `cargo fmt --check` and `cargo clippy --workspace --all-targets --profile prod -- -D warnings` clean.
