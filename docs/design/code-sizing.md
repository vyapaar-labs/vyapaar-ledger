# `vyapaar-ledger` — Risk-Aware Position Sizing Reference

Documents `crates/vyapaar-ledger/src/sizing.rs`: the risk-aware position-sizing
service hook (ADR 0032, Phase 7.4) ported onto the canonical
`vyapaar_core::sizing::RiskBudget` and `vyapaar_ledger_portfolio::PortfolioState`.

## `Portfolio`

Rich domain aggregate representing a risk-governed portfolio:

| Field | Meaning |
|-------|---------|
| `state: PortfolioState` | Internal multi-account valuation state (from `vyapaar-ledger-portfolio`). |
| `max_gross_leverage: Decimal` | Maximum gross portfolio leverage (e.g. 4.0x intraday MIS, 1.0x CNC). |
| `max_scrip_concentration_pct: Decimal` | Maximum single-scrip concentration as a fraction of equity (e.g. 0.25 = 25%). |
| `min_lot_size: Quantity` | Default instrument minimum lot size. |

Constructors: `new(state)` (defaults: 4.0x leverage, 25% concentration,
lot size 1), `with_limits(state, leverage, concentration_pct)`,
`with_min_lot_size(min_lot)`; `Deref` exposes the inner `PortfolioState`, and
`From<PortfolioState>` exists for ergonomics.

## `evaluate_risk_budget`

Pure deterministic service hook computing the `RiskBudget`:

1. **Free equity** = total uncommitted cash across all accounts.
2. **Total equity** = cash + total open notional.
3. **Gross leverage** = total gross market exposure / total equity.
4. **Leverage headroom** = `(max_gross_leverage − current_leverage) × equity`.
5. **Concentration cap** = `equity × max_scrip_concentration_pct`.
6. **Max position notional** = `min(concentration_cap, leverage_headroom)`,
   floored at zero.
7. **Max portfolio risk** = `min(risk_pct × 5, 1)`.

Returns a `vyapaar_core::sizing::RiskBudget { equity, risk_pct,
max_position_notional, max_position_qty: None, max_portfolio_risk_pct,
min_lot_size, freeze_limit_qty: None }`.

## Zero-floats & determinism

All arithmetic is exact `rust_decimal::Decimal`; no clock, no I/O — the caller
owns the `PortfolioState`'s timestamps.

## Related

- `vyapaar-core/src/sizing.rs` — canonical `RiskBudget` type.
- `vyapaar-ledger-portfolio` — `PortfolioState` (multi-account valuation).
- Tests: `crates/vyapaar-ledger/tests/repository_and_sizing_tests.rs` (4).
