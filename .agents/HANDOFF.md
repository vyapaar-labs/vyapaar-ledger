# HANDOFF — vyapaar-ledger

**Status**: Plan 0001 Phase 1 complete and merged to `main`. Batch done.  
**Date**: 2026-11-09 · Repo: `vyapaar-ledger` · Branch: `main`

## Phase / Slice Status (COMPLETE — merged)

- **Plan 0001** (Portfolio Allocation & Optimization Engine): new `vyapaar-ledger-allocation` crate implementing Phase 1 — `AllocationObjective::{MeanVariance, RiskParity}` (Hrp/BlackLitterman declared, return `NotImplemented` for later phases), `Constraint::WeightBounds`, `PortfolioState` read-adapter over `vyapaar-ledger-portfolio`, `TargetWeights` auditable output, and the pure deterministic `optimize()` entry point; 17 tests in `tests/optimize_tests.rs`; commit `b35a314`. Plan doc removed from `docs/plans/`; moved to Implemented Plans in `docs/plans/README.md`. Phases 2 (Hrp/Herc), 3 (BlackLitterman), 4 (discrete/whole-share allocation) are open future phases, explicitly out of scope for this slice.

## Decisions & Discoveries (final)

- Zero-floats policy respected: all weights, bounds, returns, and covariance entries are exact `rust_decimal::Decimal`; matrix inversion via Gaussian elimination, fixed-point risk parity, and clip-and-renormalize bound projection all run in `Decimal` (no `f32`/`f64` intermediates).
- Zero-I/O domain purity respected: no network, filesystem, or clock queries; `as_of` timestamp injected explicitly by the caller (temporal determinism).
- Bound handling: Phase 1 solves the unconstrained optimum in exact arithmetic, then projects into the box via iterative clip-and-renormalize (documented simplification; exact bound-constrained QP is a follow-on).
- Compiles cleanly on native x86_64/aarch64; workspace-wide `cargo test --profile prod`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` all green.

## Immediate Next Step

None — batch complete. No active plans remain in `docs/plans/`.

## Operating Invariants

- Canonical profile `prod`: `cargo build/check/test --profile prod`.
- Tests live in `tests/` parallel to `src/`; no in-file `#[test]` blocks.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --profile prod` green.
