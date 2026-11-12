# Plans Index — `vyapaar-ledger`

Per workspace rule `.agents/rules/gap-tracking.md`:
- **Gaps** (`docs/gaps/XXXX-*.md`): individual missing features, stubs, or rule violations.
- **Plans** (`docs/plans/XXXX-*.md`): collections of gaps.
- When implemented, plan documents are cleaned up from `docs/plans/` and recorded in this index.
- Numbering is per-repo, sequential, never reused.

The former `docs/roadmap/` directory has been dismantled into this index plus
`docs/gaps/` and `docs/design/`: the completed Tier 1B migration milestones and
the subsequent allocation/performance landings live in
[`docs/design/architecture.md`](../design/architecture.md) and
[`docs/design/implementation-record.md`](../design/implementation-record.md);
every remaining roadmap item is now a numbered gap under
[`docs/gaps/`](../gaps/README.md), grouped into the active plans below.

## Active Plans

| # | Title | Target Scope | Status |
|---|-------|--------------|--------|
| [0003](0003-allocation-phase-2-4.md) | Allocation Engine — Phases 2–4: Hrp, Black-Litterman, exact bounds, discrete allocation | `vyapaar-ledger-allocation` | Ready for Implementation |
| [0004](0004-performance-journal-edge-and-determinism.md) | Performance Journal — temporal determinism & real edge score | `vyapaar-ledger-performance` | Ready for Implementation |
| [0005](0005-settlement-and-fee-consolidation.md) | Settlement, Corporate Actions & Statutory-Fee Consolidation | `vyapaar-ledger-portfolio`, `vyapaar-ledger-journal`, `vyapaar-ledger-oms` | Proposed |

## Implemented Plans

| # | Title | Target Scope | Implemented In |
|---|-------|--------------|----------------|
| 0001 | Portfolio Allocation & Optimization Engine | `vyapaar-ledger-allocation` | Phase 1: `AllocationObjective::{MeanVariance, RiskParity}`, `Constraint::WeightBounds`, `PortfolioState` read-adapter, `TargetWeights` (custom serde), pure `optimize()` in exact `Decimal` (Gaussian elimination, fixed-point risk parity, clip-and-renormalize bounds); 17 tests in `tests/optimize_tests.rs`; commit `b35a314`. Phases 2–4 are open future phases — see gaps 0001–0004 and plan 0003. |
| 0002 | Trade performance journal: annotations & analytics engine | `vyapaar-ledger-performance` | Phases 1–3: data model (`JournalAccount`, `TradeAnnotation`, `JournalNote`, `ClosedTrade`) + `JournalStore`; `PerformanceMetrics` engine + `EdgeScore` (v1 formula); `PnlCalendar` daily P&L aggregation + `compare_accounts` / `compare_account_vs_prior`. Pure, zero-I/O, zero-float; 42 tests in `tests/`; commits `2a3a04e`, `1b7fb97`. Known v1 items — see gaps 0005–0006 and plan 0004. |
