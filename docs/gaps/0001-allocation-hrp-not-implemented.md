# Gap 0001 — Allocation `Hrp` Objective Declared but Not Implemented

**Status**: open — tracked by [plan 0003](../plans/0003-allocation-phase-2-4.md)

## What's Declared but Missing

`crates/vyapaar-ledger-allocation/src/lib.rs` declares
`AllocationObjective::Hrp` ("Hierarchical Risk Parity — Phase 2") as a first-class
serde enum variant, but `optimize()` returns
`AllocationError::NotImplemented("Hierarchical Risk Parity is a Phase 2 objective")`
for it.

## Why

- HRP (López de Prado) is the industry-standard, quants-facing alternative to
  mean-variance / risk parity and is part of the allocation roadmap
  (`docs/plans/README.md` plan 0001, Phase 2).
- The variant must exist so wire contracts and callers are stable across
  phases — but the optimization body is genuinely absent.

## How to Close It

1. Implement hierarchical clustering of the covariance matrix (correlation
  distance → single-linkage dendrogram) in pure `Decimal`.
2. Recursively allocate risk budget down the dendrogram (quasi-diagonalization
  + bisection).
3. Route `AllocationObjective::Hrp` in `optimize()` to the new solver instead
  of `NotImplemented`.
4. Add golden-vector tests in `tests/optimize_tests.rs` (Phase 1 parity with
  scipy/numpy reference vectors).

## Verification

`cargo test --workspace --profile prod` green with new HRP tests; no
`NotImplemented("Hrp")` path remains.
