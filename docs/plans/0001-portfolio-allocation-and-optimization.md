# 0001 — Portfolio Allocation & Optimization Engine

> **Status**: Proposed — not yet implemented.
> **Target Crates**: vyapaar-ledger-allocation (new crate)
> **Depends on reading**:
> - (none external)

---

## 0. Executive Summary & Objective

Fincept's Allocation + Portfolio Optimizer toolset (mean-risk optimization over ~26 risk measures, risk parity, HRP/HERC/NCO, Black-Litterman, discrete/whole-share allocation) has no Vyapaar equivalent. `vyapaar-ledger-portfolio` values and tracks positions — FIFO tax lots, marks, T+1 settlement — but it never decides *how to allocate* capital across instruments; it only reports the state of what is already held. This is the single highest-leverage gap for anyone wanting portfolio-construction features rather than just execution and valuation, so this doc proposes a new sibling crate, `vyapaar-ledger-allocation`, dedicated to that decision.

---

## 1. Context

`vyapaar-ledger` today hosts three crates: `vyapaar-ledger-journal`, `vyapaar-ledger-oms`, and `vyapaar-ledger-portfolio` ("pure portfolio valuation: positions, marks, FIFO tax lots, and T+1 settlement"). None of them answer "given my current positions, capital, and constraints, what *should* my target weights be?" — that is a distinct, pure optimization problem, not a valuation problem, and belongs in its own crate rather than bolted onto `vyapaar-ledger-portfolio`'s existing responsibility.

The new crate is a consumer of `vyapaar-ledger-portfolio`'s state (current positions as the optimization starting point) and of `vyapaar-core`'s value objects (`Price`, `Quantity`, `Money`), but it does not touch execution, order routing, or the risk gate — it produces target weights that a caller (eventually `vyapaar-exec` or a strategy layer, out of scope here) would turn into orders.

Related-but-separate gap, noted for a follow-on doc rather than folded in here: the existing `portfolio` toolset gap around attribution/scenario/benchmark analytics is currently only partially covered by `vyapaar-agent-analytics`, and is a different concern (explaining past performance vs. deciding future weights) — out of scope for `vyapaar-ledger-allocation`.

---

## 2. Component Specifications

New crate: `vyapaar-ledger-allocation`, depending on `vyapaar-core` (for `Price`/`Quantity`/`Money`) and `vyapaar-ledger-portfolio` (for current position state as the optimization starting point).

Core types:
- `AllocationObjective` enum — starts with `MeanVariance`, `RiskParity`, `Hrp`, `BlackLitterman`. Explicitly not all ~26 Fincept risk measures at once; additional objectives are additive future variants, not a redesign.
- `Constraint` set — a struct or small trait covering position bounds (min/max weight per instrument), sector/group limits, and a turnover/transaction-cost penalty term (so the optimizer can be penalized for churning an existing book rather than optimizing from a blank slate).
- `PortfolioState` — read from `vyapaar-ledger-portfolio`'s existing position/valuation types; the allocation crate does not redefine position state, it borrows the existing one as its optimization input.
- `TargetWeights` — the output type: a mapping from `InstrumentId` to a target portfolio weight (`Decimal`), plus enough metadata (as-of timestamp, objective used) to be auditable.
- `AllocationError` — covers infeasible constraint sets, singular covariance matrices, insufficient history, etc.
- Core entry point: `fn optimize(objective: AllocationObjective, constraints: &[Constraint], state: &PortfolioState) -> Result<TargetWeights, AllocationError>` — pure, zero-I/O, deterministic, unit-testable exactly like every other domain crate in this workspace (no network, no filesystem, no direct clock queries; any time-dependent input arrives as an explicit `DateTime<Utc>` parameter per the workspace's temporal-determinism invariant).

Decimal discipline: all weights, bounds, and penalty coefficients are `rust_decimal::Decimal`, never `f32`/`f64`, consistent with the workspace's zero-floats policy — including inside the mean-variance and risk-parity numerics, which will need a documented approach for operations (matrix inversion, eigen-decomposition for HRP clustering) that are typically done in floating point; this is a known hard sub-problem to resolve during Phase 1/2 implementation, not before.

---

## 3. Implementation Phases

### Phase 1: Mean-variance + risk parity
The smallest useful slice: implement `AllocationObjective::MeanVariance` and `AllocationObjective::RiskParity` with simple bound constraints only (no sector limits, no turnover penalty yet). Establishes the crate skeleton, `PortfolioState` read-adapter from `vyapaar-ledger-portfolio`, and the `optimize()` entry point's signature for all later phases to build on.

### Phase 2: HRP / HERC
Add `AllocationObjective::Hrp` (Hierarchical Risk Parity) and HERC (Hierarchical Equal Risk Contribution) clustering-based allocation, which avoids the covariance-matrix-inversion fragility of mean-variance.

### Phase 3: Black-Litterman
Add `AllocationObjective::BlackLitterman` with a new `Views` / `Priors` input type (investor views + confidence, blended with market-implied equilibrium returns) feeding the same `optimize()` entry point.

### Phase 4: Discrete / whole-share allocation
A rounding pass that converts continuous target weights into whole-lot, whole-share order sizes given available capital — more important here than it would be for a US-equities-first product, since Vyapaar trades Indian equities in whole lots rather than fractional shares. This phase defines the rounding/residual-cash-handling algorithm as the final step between `TargetWeights` and anything order-routing-shaped (order construction itself remains out of scope, owned by `vyapaar-exec`).
