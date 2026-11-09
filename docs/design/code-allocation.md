# `vyapaar-ledger-allocation` — Portfolio Allocation Reference

Documents `crates/vyapaar-ledger-allocation/src/lib.rs`: pure, zero-I/O, deterministic
portfolio allocation producing `TargetWeights` (instrument → target weight mappings).

## Phase 1 Scope

| Objective | Status | Description |
|-----------|--------|-------------|
| `AllocationObjective::MeanVariance` | Implemented | Markowitz tangency maximizing `(μᵀw − r_f) / √(wᵀΣw)` |
| `AllocationObjective::RiskParity` | Implemented | Equal risk contribution: `wᵢ(Σw)ᵢ` equal across instruments |
| `AllocationObjective::Hrp` | Declared, `NotImplemented` | Hierarchical Risk Parity (Phase 2) |
| `AllocationObjective::BlackLitterman` | Declared, `NotImplemented` | Black-Litterman blended (Phase 3) |

### Constraint

| Constraint | Purpose |
|------------|---------|
| `Constraint::WeightBounds { instrument, min, max }` | Box bound `min ≤ w ≤ max` per instrument |

## `PortfolioState` (read-adapter)

Builds optimization input from `vyapaar-ledger-portfolio`:
- `from_portfolio(portfolio, marks, as_of)` — universe = union of held positions; `current_weights` = market-value-normalized weights.
- `with_expected_returns()` / `with_covariance()` — attach estimates (from `vyapaar_core::quant` primitives, caller-provided).

## `TargetWeights` (output)

- `as_of`, `objective`, `weights: BTreeMap<InstrumentId, Decimal>` (sums to 1 within decimal tolerance).
- Custom JSON serialization: emitted as sorted array of `{instrument, weight}` entries (since `InstrumentId` is a struct, not a valid JSON object key).

## Zero-Floats & Determinism

- All weights, bounds, returns, covariance entries: exact `rust_decimal::Decimal`.
- Matrix inversion via Gaussian elimination in pure `Decimal` (no `f64`).
- Fixed-point risk parity, clip-and-renormalize bound projection — all in `Decimal`.
- No network, filesystem, or clock queries; `as_of` injected explicitly by caller.

## Bound Handling (Phase 1 Simplification)

Solves the unconstrained optimum exactly, then projects into the box via iterative
clip-and-renormalize: clip each weight to `[min, max]`, re-scale remaining in-bound
weights to re-sum to 1, repeat until stable. Exact bound-constrained QP is a follow-on.

## Related

- `vyapaar-ledger-portfolio` — `PortfolioState`, `Marks` (read-adapter source).
- `vyapaar-core::quant` — returns/covariance primitives (source for `expected_returns`/`covariance`).
- Gap 0001 / Plan 0002 — call-site consolidation to delegate downstream quant math here.
