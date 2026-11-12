# `vyapaar-ledger-allocation` — Portfolio Allocation Reference

Documents `crates/vyapaar-ledger-allocation/src/lib.rs`: pure, zero-I/O,
deterministic portfolio allocation producing `TargetWeights` (instrument →
target weight mappings). It never places an order — a caller (eventually
`vyapaar-exec` or a strategy layer) would turn the weights into orders.

## Phase 1 Scope

| Objective | Status | Description |
|-----------|--------|-------------|
| `AllocationObjective::MeanVariance` | Implemented | Markowitz tangency maximizing `(μᵀw − r_f) / √(wᵀΣw)` — solves `Σ·w = μ − r_f` by Gaussian elimination, normalizes, projects into the box. |
| `AllocationObjective::RiskParity` | Implemented | Equal risk contribution: `wᵢ(Σw)ᵢ` equal across instruments — damped fixed-point iteration (α = 0.5). |
| `AllocationObjective::Hrp` | Declared, `NotImplemented` | Hierarchical Risk Parity — Phase 2, [gap 0001](../gaps/0001-allocation-hrp-not-implemented.md). |
| `AllocationObjective::BlackLitterman` | Declared, `NotImplemented` | Black-Litterman blended — Phase 3, [gap 0002](../gaps/0002-allocation-black-litterman-not-implemented.md). |

### Constraint

| Constraint | Purpose |
|------------|---------|
| `Constraint::WeightBounds { instrument, min, max }` | Box bound `min ≤ w ≤ max` per instrument (defaults `(0, 1)` long-only). |

Infeasible constraint sets (`min > max`, `sum(min) > 1`, `sum(max) < 1`),
singular covariance, missing return/covariance entries, and empty universes are
rejected with `AllocationError`.

## `PortfolioState` (read-adapter)

Builds optimization input from `vyapaar-ledger-portfolio`:

- `from_portfolio(portfolio, marks, as_of)` — universe = union of held
  positions; `current_weights` = market-value-normalized weights.
- `with_expected_returns()` / `with_covariance()` — attach estimates (from
  `vyapaar_core::quant` primitives, caller-provided).

## `TargetWeights` (output)

- `as_of`, `objective`, `weights: BTreeMap<InstrumentId, Decimal>` (sums to 1
  within `1e-9` tolerance).
- Custom JSON serialization: emitted as a sorted array of `{instrument, weight}`
  entries (an `InstrumentId` is a struct, not a valid JSON object key).

## Zero-floats & determinism

All weights, bounds, returns, and covariance entries are exact
`rust_decimal::Decimal`. Matrix inversion (Gaussian elimination with partial
pivoting), risk-parity fixed point, and bound projection all run in `Decimal`
(no `f32`/`f64` even as intermediates). No network, filesystem, or clock
queries; `as_of` is injected explicitly by the caller.

## Bound handling (documented Phase 1 simplification)

`optimize()` solves the unconstrained optimum in exact arithmetic, then
projects into the box via iterative clip-and-renormalize: clip each weight to
`[min, max]`, re-scale the strictly-in-bound weights so the portfolio re-sums
to 1, repeat until stable (or fail with `InfeasibleConstraints` if nothing
free remains). An exact bound-constrained QP solver is a follow-on — see
[gap 0003](../gaps/0003-allocation-exact-bound-constrained-qp.md).
Discrete/whole-share allocation is Phase 4 — see
[gap 0004](../gaps/0004-allocation-discrete-whole-share.md).

## Related

- `vyapaar-ledger-portfolio` — `PortfolioState`, `Marks` (read-adapter source).
- `vyapaar-core::quant` — returns/covariance primitives (source for
  `expected_returns`/`covariance`).
- Phase 2–4 work is planned in
  [`docs/plans/0003-allocation-phase-2-4.md`](../plans/0003-allocation-phase-2-4.md).
- Tests: `crates/vyapaar-ledger-allocation/tests/optimize_tests.rs` (17).
