# Plans Index — `vyapaar-ledger`

Per workspace rule `.agents/rules/gap-tracking.md`:
- Gaps are individual features (`docs/gaps/XXXX-*.md`).
- Plans are collections of gaps (`docs/plans/XXXX-*.md`).
- When implemented, plan documents are cleaned up from `docs/plans/` and recorded in this index.

## Active Plans

## Implemented Plans

| # | Title | Target Scope | Implemented In |
|---|-------|--------------|----------------|
| 0001 | Portfolio Allocation & Optimization Engine | `vyapaar-ledger-allocation` | Phase 1: new `vyapaar-ledger-allocation` crate with `AllocationObjective::{MeanVariance, RiskParity}`, `Constraint::WeightBounds`, `PortfolioState` read-adapter over `vyapaar-ledger-portfolio`, `TargetWeights` output, and the pure `optimize()` entry point (`src/lib.rs`); 17 tests in `tests/optimize_tests.rs`; commits `b35a314`. Phases 2 (Hrp/Herc), 3 (BlackLitterman), and 4 (discrete/whole-share allocation) are open future phases, not part of this slice. |
| 0002 | Trade performance journal: annotations & analytics engine | New `vyapaar-ledger-performance` crate | Phase 1: data model (`JournalAccount`, `TradeAnnotation`, `JournalNote`, `ClosedTrade`) + `JournalStore`; Phase 2: `PerformanceMetrics` engine + `EdgeScore` composite (v1 formula); Phase 3: `PnlCalendar` daily P&L aggregation + `compare_accounts`/`compare_account_vs_prior`. Pure, zero-I/O, zero-float, `rust_decimal::Decimal` only; 40 tests in `tests/`; `cargo fmt/clippy -- -D warnings` and `cargo test` clean. |

