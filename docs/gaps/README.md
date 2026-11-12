# Gap Index — `vyapaar-ledger`

Per workspace rule `.agents/rules/gap-tracking.md`:
- **Gaps** (`docs/gaps/XXXX-*.md`): individual missing features, stubs, or rule violations.
- **Plans** (`docs/plans/XXXX-*.md`): collections of gaps.
- Numbering is per-repo, sequential, never reused.

Dismantled from the former `docs/roadmap/` directory: every completed milestone
now lives in [`docs/design/`](../design/architecture.md) +
[`docs/design/implementation-record.md`](../design/implementation-record.md);
the remaining future work is enumerated below.

## Open Gaps

| # | Title | Severity | Status | Plan |
|---|-------|----------|--------|------|
| [0001](0001-allocation-hrp-not-implemented.md) | Allocation `Hrp` objective declared but not implemented | Medium | Open | [0003](../plans/0003-allocation-phase-2-4.md) |
| [0002](0002-allocation-black-litterman-not-implemented.md) | Allocation `BlackLitterman` objective declared but not implemented | Medium | Open | [0003](../plans/0003-allocation-phase-2-4.md) |
| [0003](0003-allocation-exact-bound-constrained-qp.md) | Allocation bound handling is clip-and-renormalize, not exact bound-constrained QP | Low | Open | [0003](../plans/0003-allocation-phase-2-4.md) |
| [0004](0004-allocation-discrete-whole-share.md) | Discrete / whole-share allocation not implemented | Low | Open | [0003](../plans/0003-allocation-phase-2-4.md) |
| [0005](0005-performance-metrics-clock-query.md) | Performance `MetricsInput::new()` mints `as_of` via `Utc::now()` | High | Open | [0004](../plans/0004-performance-journal-edge-and-determinism.md) |
| [0006](0006-performance-edge-score-v1-placeholder.md) | Performance edge score is a v1 placeholder formula | Low | Open | [0004](../plans/0004-performance-journal-edge-and-determinism.md) |
| [0007](0007-portfolio-hardcoded-nse-calendar.md) | Portfolio `ClearingCalendar::new()` hardcodes NSE holidays 2024–2026 | Medium | Open | [0005](../plans/0005-settlement-and-fee-consolidation.md) |
| [0008](0008-portfolio-corporate-action-per-lot-cost-basis.md) | Corporate-action adjustments are aggregate-level, not per-lot | Medium | Open | [0005](../plans/0005-settlement-and-fee-consolidation.md) |
| [0009](0009-journal-per-segment-fee-tables.md) | Journal exchange turnover fee tables fall back to NSE cash-segment rate for NFO/MCX/CDS | Medium | Open | [0005](../plans/0005-settlement-and-fee-consolidation.md) |
| [0010](0010-journal-golden-framework-legacy-fees.md) | Golden contract-note framework runs on the legacy floor-rounded fee track | Low | Open | [0005](../plans/0005-settlement-and-fee-consolidation.md) |
| [0011](0011-oms-context-types-unwired.md) | OMS context types (`OmsConfig`, `MarketData`, `PortfolioRiskState`) defined but not wired into validation | Medium | Open | [0005](../plans/0005-settlement-and-fee-consolidation.md) |

## Closed Gaps

| # | Title | Closed In |
|---|-------|-----------|
| — | — | (none yet) |
