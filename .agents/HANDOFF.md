# HANDOFF — vyapaar-ledger

**Status**: Docs restructured — roadmap dismantled into `design` / `gaps` / `plans`. Batch done.
**Date**: 2026-11-12 · Repo: `vyapaar-ledger` · Branch: `main`

## Phase / Slice Status (COMPLETE)

- **Docs restructure**: `docs/roadmap/` deleted. Completed milestones moved to
  `docs/design/implementation-record.md` (+ refreshed `docs/design/architecture.md`
  and per-module references); every remaining roadmap item filed as a numbered
  gap under `docs/gaps/` (0001–0011) grouped into active plans 0003–0005.
- **Verification**: `cargo test --workspace --profile prod` green — 182 tests,
  zero failures; no `#[cfg(test)]` in `crates/*/src/`.

## Decisions & Discoveries (final)

- Gaps filed (per `.agents/rules/gap-tracking.md`, numbering 0001+):
  - Allocation: `Hrp` (0001), `BlackLitterman` (0002), exact bound-constrained
    QP (0003), discrete/whole-share allocation (0004) → plan 0003.
  - Performance: `MetricsInput::new()` mints `as_of: Utc::now()` — the only
    wall-clock query in the workspace's `crates/*/src/` (0005); v1 placeholder
    edge-score formula (0006) → plan 0004.
  - Portfolio: hardcoded NSE 2024–2026 holiday table (0007); aggregate-level
    (not per-lot) corporate-action adjustments (0008) → plan 0005.
  - Journal: NFO/MCX/CDS turnover falls back to NSE cash-segment rate (0009);
    golden framework runs on the legacy floor-rounded fee track (0010) → plan 0005.
  - OMS: `OmsConfig`/`MarketData`/`PortfolioRiskState` declared but not wired
    into validation (0011) → plan 0005.
- New design references: `code-contract-notes.md`, `code-sizing.md`,
  `code-performance.md`; `code-oms/portfolio/journal/repository/allocation.md`
  refreshed to match source; `code-migration-map.md` now uses `--profile prod`
  and points to `implementation-record.md`.
- Test totals per crate: facade 4, OMS 25, portfolio 36, journal 58
  (incl. proptest invariants), allocation 17, performance 42.

## Immediate Next Step

None — docs batch complete. No active roadmap; active plans 0003–0005 are
future work (no plan doc leaves `docs/plans/` until implementation starts).

## Operating Invariants

- Canonical profile `prod`: `cargo build/check/test --profile prod`.
- Tests live in `tests/` parallel to `src/`; no in-file `#[test]` blocks.
- Zero-I/O, zero floats (`rust_decimal::Decimal`), caller-injected timestamps —
  the `Utc::now()` in `metrics.rs` is gap 0005, do not re-introduce.
- `cargo fmt --check`, `cargo clippy --workspace --all-targets --profile prod -- -D warnings`,
  `cargo test --workspace --profile prod` green.
