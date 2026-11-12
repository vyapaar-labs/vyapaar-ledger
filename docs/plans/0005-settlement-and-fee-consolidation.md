# Plan 0005 — Settlement, Corporate Actions & Statutory-Fee Consolidation

**Status**: Proposed

**Scope**: `vyapaar-ledger-portfolio`, `vyapaar-ledger-journal`, `vyapaar-ledger-oms`

**Gaps covered**:
- [0007](../gaps/0007-portfolio-hardcoded-nse-calendar.md) — hardcoded NSE holiday table
- [0008](../gaps/0008-portfolio-corporate-action-per-lot-cost-basis.md) — aggregate-level corporate-action adjustments
- [0009](../gaps/0009-journal-per-segment-fee-tables.md) — NFO/MCX/CDS turnover fee fallback
- [0010](../gaps/0010-journal-golden-framework-legacy-fees.md) — golden framework on the legacy fee track
- [0011](../gaps/0011-oms-context-types-unwired.md) — OMS context types not wired into validation

## 0. Executive Summary & Objective

Hardening pass across the three settlement/tax/risk surfaces of the domain
heart: move hardcoded and approximate reference data out of the domain crates,
make statutory fees single-track, and wire the OMS context canvas into actual
validation. Every change preserves the zero-I/O, zero-float, and
temporal-determinism invariants.

## 1. Context & Gaps Covered

The portfolio crate seeds a static NSE holiday table (0007) and applies
corporate actions at the aggregate level instead of per `TaxLot` (0008). The
journal's `compute_charges` bills every derivative segment at the NSE
cash-segment rate (0009) and its golden suite validates the legacy floor-rounded
fee track rather than the canonical one (0010). The OMS declares
`OmsConfig`/`MarketData`/`PortfolioRiskState` but enforces none of them (0011).

## 2. Component Specifications & Phases

### Phase 1 — Portfolio: reference data & per-lot corporate actions

- Remove the hardcoded holiday table; `ClearingCalendar::new()` becomes an
  empty (weekend-only) calendar, real holidays injected via `with_holidays`
  from `vyapaar-data` / the backtest `NseTradingCalendar` (gap 0007).
- Add per-lot adjustments for split/bonus/demerger on `TaxLot` quantity,
  entry price, and remaining quantity; keep aggregate helpers as projections
  (gap 0008).

### Phase 2 — Journal: per-segment fees & single-track golden framework

- Add NFO/MCX/CDS turnover fee tables fed from an external finance/data
  source; extend `compute_charges`; remove the NSE fallback (gap 0009).
- Switch `GoldenTestRunner` onto `compute_charges`; re-baseline fixtures;
  retire the legacy floor-rounded methods (gap 0010).

### Phase 3 — OMS: wire the context canvas into validation

- Add `OmsConfig`-driven checks: `max_notional_per_order`,
  configured `max_quantity_per_order`, and `price_collar_bps` against
  `MarketData`.
- Maintain `OmsSnapshot` counters or remove the type until a consumer exists
  (gap 0011).

## 3. Acceptance Criteria

- `grep -rn "Utc::now" crates/*/src/` returns nothing (unchanged invariant).
- `cargo test --workspace --profile prod` green with new per-lot,
  per-segment, golden-canonical, and OMS-collar tests.
- `cargo fmt --check` and `cargo clippy --workspace --all-targets --profile prod -- -D warnings` clean.
- No production `src/` code calls the legacy fee track; no hardcoded holiday
  table remains in the domain crate.
