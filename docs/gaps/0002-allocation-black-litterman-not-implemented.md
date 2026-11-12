# Gap 0002 — Allocation `BlackLitterman` Objective Declared but Not Implemented

**Status**: open — tracked by [plan 0003](../plans/0003-allocation-phase-2-4.md)

## What's Declared but Missing

`crates/vyapaar-ledger-allocation/src/lib.rs` declares
`AllocationObjective::BlackLitterman` ("Black-Litterman blended allocation —
Phase 3") as a first-class serde enum variant, but `optimize()` returns
`AllocationError::NotImplemented("Black-Litterman is a Phase 3 objective")` for it.

## Why

- Black-Litterman blends market-implied returns (reverse-optimized from a
  benchmark weight vector) with investor views — a core allocator feature on
  the roadmap (`docs/plans/README.md` plan 0001, Phase 3).
- The variant is present for wire stability; the optimizer body is absent.

## How to Close It

1. Add the reverse-optimization step (`τ·Σ·w_mkt`) to derive prior returns.
2. Add the view-posterior blending (pick matrix `P`, view covariance `Ω`,
  posterior `( (τΣ)⁻¹ + PᵀΩ⁻¹P )⁻¹`).
3. Route `AllocationObjective::BlackLitterman` in `optimize()` to the new
  solver (the view inputs will need a new input type or an extension of
  `PortfolioState`).
4. Add golden-vector tests in `tests/optimize_tests.rs`.

## Verification

`cargo test --workspace --profile prod` green with new Black-Litterman tests;
no `NotImplemented("Black-Litterman")` path remains.
