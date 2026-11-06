//! # व्यापार・आवंटन vyapaar-ledger-allocation
//!
//! Pure portfolio **allocation** — the decision of *what target weights to
//! hold* — as a distinct sibling of `vyapaar-ledger-portfolio`, which only
//! values and tracks what is already held (FIFO tax lots, marks, T+1
//! settlement). This crate is deliberately pure, zero-I/O and deterministic:
//! no network, no filesystem, no clock queries; every time-dependent input
//! arrives as an explicit `as_of` timestamp.
//!
//! It produces [`TargetWeights`] — a mapping from [`InstrumentId`] to a target
//! portfolio weight — that a caller (eventually `vyapaar-exec` or a strategy
//! layer, out of scope here) would turn into orders. It never places an order
//! itself.
//!
//! # Phase 1 scope (this release)
//!
//! [`AllocationObjective::MeanVariance`] and [`AllocationObjective::RiskParity`]
//! with simple per-instrument [`Constraint::WeightBounds`] only (no sector
//! limits, no turnover penalty). `Hrp` / `BlackLitterman` are declared variants
//! that return [`AllocationError::NotImplemented`] — they land in later
//! phases.
//!
//! # Zero-floats discipline
//!
//! All weights, bounds, returns, and covariance entries are exact
//! `rust_decimal::Decimal`. The numerics — Gaussian-elimination matrix
//! inversion, fixed-point risk parity, clip-and-renormalize bound projection —
//! are carried out entirely in `Decimal`, so no `f32`/`f64` enters this crate
//! even as an intermediate.
//!
//! # Bound handling (documented Phase 1 simplification)
//!
//! Bound-constrained quadratic programming is a known hard sub-problem in pure
//! `Decimal`. Phase 1 therefore solves the *unconstrained* optimum in exact
//! arithmetic and then projects it into the box via iterative
//! clip-and-renormalize: clip each weight to its `[min, max]`, then re-scale
//! the weights that are still strictly inside their bounds so the portfolio
//! re-sums to 1, repeating until stable. An exact bound-constrained QP solver
//! is a follow-on refinement, not part of this slice.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

pub use vyapaar_core::{InstrumentId, Price, Quantity};

/// Convergence tolerance for the numerical loops (risk parity fixed point,
/// clip-and-renormalize): `1e-9`.
const EPS: Decimal = Decimal::from_parts(1, 0, 0, false, 9); // 0.000000001

/// The optimization objective.
///
/// All four planned objectives are declared now; only `MeanVariance` and
/// `RiskParity` are implemented in Phase 1, and additional variants are
/// additive future work rather than a redesign of the enum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AllocationObjective {
    /// Markowitz tangency portfolio maximizing `(μᵀw − r_f) / √(wᵀΣw)`.
    MeanVariance {
        /// The risk-free rate subtracted from expected returns.
        risk_free_rate: Decimal,
    },
    /// Equal risk contribution: `wᵢ(Σw)ᵢ` equal across all instruments.
    RiskParity,
    /// Hierarchical Risk Parity — Phase 2.
    Hrp,
    /// Black-Litterman blended allocation — Phase 3.
    BlackLitterman,
}

/// A constraint on the optimization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Constraint {
    /// Box bound on a single instrument's target weight: `min ≤ w ≤ max`.
    WeightBounds {
        /// The constrained instrument.
        instrument: InstrumentId,
        /// Minimum weight (0 = long-only; negative allows a short).
        min: Decimal,
        /// Maximum weight.
        max: Decimal,
    },
}

/// The optimization input: the instrument universe, the current book as
/// weights, and the return/covariance estimates the optimizer works from.
///
/// `expected_returns` and `covariance` come from return history (e.g. the
/// [`vyapaar_core::quant`] primitives); `current_weights` / the instrument
/// universe come from the caller's held positions via
/// [`PortfolioState::from_portfolio`] (the read-adapter over
/// `vyapaar-ledger-portfolio`).
#[derive(Debug, Clone, PartialEq)]
pub struct PortfolioState {
    /// As-of timestamp injected by the caller (temporal determinism).
    pub as_of: DateTime<Utc>,
    /// The ordered instrument universe (index `i` ↔ position `i` in every
    /// vector-shaped input below).
    pub instruments: Vec<InstrumentId>,
    /// Current portfolio weights, used as the optimization starting point.
    pub current_weights: BTreeMap<InstrumentId, Decimal>,
    /// Expected (annualized) per-instrument return, used by `MeanVariance`.
    pub expected_returns: BTreeMap<InstrumentId, Decimal>,
    /// The covariance matrix, `covariance[a][b]`, indexed by instrument.
    pub covariance: BTreeMap<InstrumentId, BTreeMap<InstrumentId, Decimal>>,
}

impl PortfolioState {
    /// Reads the optimization input from a `vyapaar-ledger-portfolio` state
    /// and its marks: the ordered universe is the union of every held
    /// position across all accounts, and `current_weights` are each position's
    /// market value (`marks × quantity`) normalized to the portfolio total.
    ///
    /// `expected_returns` and `covariance` cannot come from the portfolio
    /// crate (it tracks positions, not return history) and must be attached
    /// with [`PortfolioState::with_expected_returns`] /
    /// [`PortfolioState::with_covariance`].
    pub fn from_portfolio(
        portfolio: &vyapaar_ledger_portfolio::PortfolioState,
        marks: &vyapaar_ledger_portfolio::Marks,
        as_of: DateTime<Utc>,
    ) -> Self {
        let mut values: BTreeMap<InstrumentId, Decimal> = BTreeMap::new();
        for account in portfolio.accounts.values() {
            for pos in &account.positions {
                let market = marks
                    .marks
                    .get(&pos.instrument)
                    .copied()
                    .unwrap_or(Price::ZERO);
                let value = market.value() * pos.quantity.value();
                *values.entry(pos.instrument.clone()).or_default() += value;
            }
        }
        let total: Decimal = values.values().copied().sum();
        let instruments = values.keys().cloned().collect::<Vec<_>>();
        let current_weights = instruments
            .iter()
            .map(|id| {
                let v = values[id];
                let w = if total.is_zero() {
                    Decimal::ZERO
                } else {
                    v / total
                };
                (id.clone(), w)
            })
            .collect();
        Self {
            as_of,
            instruments,
            current_weights,
            expected_returns: BTreeMap::new(),
            covariance: BTreeMap::new(),
        }
    }

    /// Attaches the expected-return estimates (required for `MeanVariance`).
    pub fn with_expected_returns(
        mut self,
        expected_returns: BTreeMap<InstrumentId, Decimal>,
    ) -> Self {
        self.expected_returns = expected_returns;
        self
    }

    /// Attaches the covariance matrix.
    pub fn with_covariance(
        mut self,
        covariance: BTreeMap<InstrumentId, BTreeMap<InstrumentId, Decimal>>,
    ) -> Self {
        self.covariance = covariance;
        self
    }
}

/// The auditable output of an allocation run.
///
/// JSON serialization is custom: an `InstrumentId` is a struct, so the
/// `InstrumentId → Decimal` weight map cannot use JSON object keys. It is
/// emitted as a sorted array of `{ instrument, weight }` entries instead.
#[derive(Debug, Clone, PartialEq)]
pub struct TargetWeights {
    /// As-of timestamp of the allocation.
    pub as_of: DateTime<Utc>,
    /// The objective that produced these weights.
    pub objective: AllocationObjective,
    /// Target weight per instrument, summing to 1 (within decimal tolerance).
    pub weights: BTreeMap<InstrumentId, Decimal>,
}

/// One serialized weight entry (see [`TargetWeights`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct WeightEntry {
    instrument: InstrumentId,
    weight: Decimal,
}

impl serde::Serialize for TargetWeights {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("TargetWeights", 3)?;
        state.serialize_field("as_of", &self.as_of)?;
        state.serialize_field("objective", &self.objective)?;
        let entries: Vec<WeightEntry> = self
            .weights
            .iter()
            .map(|(instrument, weight)| WeightEntry {
                instrument: instrument.clone(),
                weight: *weight,
            })
            .collect();
        state.serialize_field("weights", &entries)?;
        state.end()
    }
}

impl<'de> serde::Deserialize<'de> for TargetWeights {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct TargetWeightsWire {
            as_of: DateTime<Utc>,
            objective: AllocationObjective,
            weights: Vec<WeightEntry>,
        }
        let wire = TargetWeightsWire::deserialize(deserializer)?;
        let weights = wire
            .weights
            .into_iter()
            .map(|entry| (entry.instrument, entry.weight))
            .collect();
        Ok(TargetWeights {
            as_of: wire.as_of,
            objective: wire.objective,
            weights,
        })
    }
}

/// Allocation errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AllocationError {
    /// The constraint set cannot be satisfied (e.g. `min > max`, or the
    /// mandatory weight bounds sum outside `[0, 1]`).
    #[error("infeasible constraint set: {0}")]
    InfeasibleConstraints(String),

    /// The covariance matrix is singular (or not positive-definite) and the
    /// required inverse / solver step cannot be completed.
    #[error("covariance matrix is singular or not positive-definite")]
    SingularCovariance,

    /// Not enough return history was supplied to estimate the inputs.
    #[error("insufficient history: needed at least {needed} observations, got {got}")]
    InsufficientHistory {
        /// Observations required.
        needed: usize,
        /// Observations available.
        got: usize,
    },

    /// The objective is declared but not implemented in this phase.
    #[error("objective not implemented: {0}")]
    NotImplemented(String),

    /// The input is malformed (missing covariance/return entries, unknown
    /// instruments in constraints, empty universe).
    #[error("invalid allocation input: {0}")]
    InvalidArgument(String),
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Solves the allocation and returns auditable target weights.
///
/// Pure, zero-I/O and deterministic: identical inputs produce identical
/// weights. Weights always sum to 1 within decimal tolerance and respect every
/// supplied [`Constraint`].
pub fn optimize(
    objective: AllocationObjective,
    constraints: &[Constraint],
    state: &PortfolioState,
) -> Result<TargetWeights, AllocationError> {
    let as_of = state.as_of;
    let weights = match &objective {
        AllocationObjective::MeanVariance { risk_free_rate } => {
            mean_variance(*risk_free_rate, constraints, state)?
        }
        AllocationObjective::RiskParity => risk_parity(constraints, state)?,
        AllocationObjective::Hrp => {
            return Err(AllocationError::NotImplemented(
                "Hierarchical Risk Parity is a Phase 2 objective".into(),
            ));
        }
        AllocationObjective::BlackLitterman => {
            return Err(AllocationError::NotImplemented(
                "Black-Litterman is a Phase 3 objective".into(),
            ));
        }
    };
    Ok(TargetWeights {
        as_of,
        objective,
        weights,
    })
}

// ---------------------------------------------------------------------------
// Input preparation shared by both implemented objectives
// ---------------------------------------------------------------------------

/// Column-vector view of the optimization input.
struct Input {
    instruments: Vec<InstrumentId>,
    /// `(min, max)` weight bound per instrument (defaults `(0, 1)`).
    bounds: Vec<(Decimal, Decimal)>,
    /// Expected return per instrument.
    expected_returns: Vec<Decimal>,
    /// `n × n` covariance matrix.
    covariance: Vec<Vec<Decimal>>,
}

fn prepare_input(
    constraints: &[Constraint],
    state: &PortfolioState,
) -> Result<Input, AllocationError> {
    if state.instruments.is_empty() {
        return Err(AllocationError::InvalidArgument(
            "instrument universe is empty".into(),
        ));
    }
    let n = state.instruments.len();

    let mut min = vec![Decimal::ZERO; n];
    let mut max = vec![Decimal::ONE; n];
    for constraint in constraints {
        let Constraint::WeightBounds {
            instrument,
            min: lo,
            max: hi,
        } = constraint;
        let idx = state
            .instruments
            .iter()
            .position(|id| id == instrument)
            .ok_or_else(|| {
                AllocationError::InvalidArgument(format!(
                    "constraint references instrument {instrument} which is not in the universe"
                ))
            })?;
        if lo > hi {
            return Err(AllocationError::InfeasibleConstraints(format!(
                "weight bounds for {instrument}: min {lo} > max {hi}"
            )));
        }
        min[idx] = *lo;
        max[idx] = *hi;
    }

    // Feasibility: the mandatory weight box must be able to contain a
    // portfolio that sums to 1.
    if min.iter().copied().sum::<Decimal>() > Decimal::ONE {
        return Err(AllocationError::InfeasibleConstraints(
            "sum of minimum weight bounds exceeds 1".into(),
        ));
    }
    if max.iter().copied().sum::<Decimal>() < Decimal::ONE {
        return Err(AllocationError::InfeasibleConstraints(
            "sum of maximum weight bounds is below 1".into(),
        ));
    }

    let mut expected_returns = Vec::with_capacity(n);
    for instrument in &state.instruments {
        expected_returns.push(
            state
                .expected_returns
                .get(instrument)
                .copied()
                .unwrap_or(Decimal::ZERO),
        );
    }

    let mut covariance = Vec::with_capacity(n);
    for instrument in &state.instruments {
        let row = state.covariance.get(instrument).ok_or_else(|| {
            AllocationError::InvalidArgument(format!("missing covariance row for {instrument}"))
        })?;
        let mut row_vec = Vec::with_capacity(n);
        for other in &state.instruments {
            let value = row.get(other).copied().ok_or_else(|| {
                AllocationError::InvalidArgument(format!(
                    "missing covariance entry for ({instrument}, {other})"
                ))
            })?;
            row_vec.push(value);
        }
        covariance.push(row_vec);
    }

    Ok(Input {
        instruments: state.instruments.clone(),
        bounds: min.into_iter().zip(max).collect(),
        expected_returns,
        covariance,
    })
}

/// Projects `weights` into the box `[min, max]` while keeping `Σw = 1`, via
/// iterative clip-and-renormalize (see the module docs for the Phase 1 note).
fn apply_bounds(
    weights: &mut [Decimal],
    bounds: &[(Decimal, Decimal)],
) -> Result<(), AllocationError> {
    let n = weights.len();
    for _ in 0..(n + 4) {
        // 1. Clip to bounds.
        for (w, (lo, hi)) in weights.iter_mut().zip(bounds) {
            if *w < *lo {
                *w = *lo;
            }
            if *w > *hi {
                *w = *hi;
            }
        }

        // 2. If we already sum to 1, we are done.
        let sum: Decimal = weights.iter().copied().sum();
        if (sum - Decimal::ONE).abs() <= EPS {
            return Ok(());
        }

        // 3. Re-scale the weights that are strictly inside their bounds so the
        //    portfolio re-sums to 1; pinned weights (at a bound) stay put.
        let mut free_sum = Decimal::ZERO;
        for (w, (lo, hi)) in weights.iter().zip(bounds) {
            if *w > *lo + EPS && *w < *hi - EPS {
                free_sum += *w;
            }
        }
        if free_sum <= EPS {
            // Nothing free left to move; the remaining gap is infeasible.
            return Err(AllocationError::InfeasibleConstraints(
                "cannot re-normalize weights to sum to 1 within the given bounds".into(),
            ));
        }
        let pinned: Decimal = weights.iter().copied().sum::<Decimal>() - free_sum;
        let factor = (Decimal::ONE - pinned) / free_sum;
        for (w, (lo, hi)) in weights.iter_mut().zip(bounds) {
            if *w > *lo + EPS && *w < *hi - EPS {
                *w *= factor;
            }
        }
    }
    Err(AllocationError::InfeasibleConstraints(
        "clip-and-renormalize did not converge".into(),
    ))
}

// ---------------------------------------------------------------------------
// Mean-Variance (tangency portfolio)
// ---------------------------------------------------------------------------

fn mean_variance(
    risk_free_rate: Decimal,
    constraints: &[Constraint],
    state: &PortfolioState,
) -> Result<BTreeMap<InstrumentId, Decimal>, AllocationError> {
    let input = prepare_input(constraints, state)?;
    for instrument in &input.instruments {
        if !state.expected_returns.contains_key(instrument) {
            return Err(AllocationError::InvalidArgument(format!(
                "missing expected return for {instrument} (MeanVariance requires it)"
            )));
        }
    }

    // Solve Σ·w = μ − r_f for the tangency portfolio, then normalize.
    let a: Vec<Decimal> = input
        .expected_returns
        .iter()
        .map(|m| *m - risk_free_rate)
        .collect();
    let mut w =
        solve_linear_system(&input.covariance, &a).ok_or(AllocationError::SingularCovariance)?;

    let sum: Decimal = w.iter().copied().sum();
    if sum.is_zero() {
        return Err(AllocationError::SingularCovariance);
    }
    for wi in &mut w {
        *wi /= sum;
    }

    apply_bounds(&mut w, &input.bounds)?;
    Ok(input.instruments.into_iter().zip(w).collect())
}

// ---------------------------------------------------------------------------
// Risk Parity (equal risk contribution)
// ---------------------------------------------------------------------------

fn risk_parity(
    constraints: &[Constraint],
    state: &PortfolioState,
) -> Result<BTreeMap<InstrumentId, Decimal>, AllocationError> {
    let input = prepare_input(constraints, state)?;
    let n = input.instruments.len();
    let inv_n = Decimal::ONE / Decimal::from(n as u64);

    // Damped fixed-point iteration for wᵢ(Σw)ᵢ = const, Σw = 1:
    //   F(w) = normalize(1 ⊘ (Σ·w))   (element-wise reciprocal)
    //   w ← (1 − α)w + α·F(w), α = 0.5
    // Damping is required — the undamped map 2-cycles even for a diagonal
    // covariance (e.g. diag(0.04, 0.16) alternates [0.8, 0.2] ↔ [0.5, 0.5]).
    let mut w = vec![inv_n; n];
    for _ in 0..512 {
        let sigma_w = matvec(&input.covariance, &w);

        // Element-wise reciprocal of the marginal risk vector.
        let mut recip = Vec::with_capacity(n);
        let mut beta = Decimal::ZERO;
        let mut feasible = true;
        for &v in &sigma_w {
            if v <= Decimal::ZERO {
                // Marginal risk must be positive for risk parity; a
                // non-positive-definite covariance is a caller error.
                feasible = false;
                break;
            }
            let r = Decimal::ONE / v;
            recip.push(r);
            beta += r;
        }
        if !feasible {
            return Err(AllocationError::SingularCovariance);
        }

        let mut next = Vec::with_capacity(n);
        let mut max_delta = Decimal::ZERO;
        for (i, r) in recip.iter().enumerate() {
            let target = *r / beta;
            let delta = (target - w[i]).abs();
            if delta > max_delta {
                max_delta = delta;
            }
            // Damped update with α = 0.5.
            next.push(w[i] + (target - w[i]) * Decimal::new(5, 1));
        }
        w = next;
        if max_delta <= EPS {
            break;
        }
    }

    apply_bounds(&mut w, &input.bounds)?;
    Ok(input.instruments.into_iter().zip(w).collect())
}

// ---------------------------------------------------------------------------
// Linear algebra helpers (pure Decimal)
// ---------------------------------------------------------------------------

/// `matrix · vector` product.
fn matvec(matrix: &[Vec<Decimal>], vector: &[Decimal]) -> Vec<Decimal> {
    matrix
        .iter()
        .map(|row| {
            row.iter()
                .zip(vector)
                .map(|(a, b)| *a * *b)
                .sum::<Decimal>()
        })
        .collect()
}

/// Solves `matrix · x = rhs` for a square matrix by Gaussian elimination with
/// partial pivoting, entirely in `Decimal`. Returns `None` for a singular
/// matrix.
fn solve_linear_system(matrix: &[Vec<Decimal>], rhs: &[Decimal]) -> Option<Vec<Decimal>> {
    let n = rhs.len();
    let mut m = matrix.to_vec();
    let mut b = rhs.to_vec();

    for col in 0..n {
        // Partial pivot: row with the largest absolute value in this column.
        let mut pivot = col;
        for row in (col + 1)..n {
            if m[row][col].abs() > m[pivot][col].abs() {
                pivot = row;
            }
        }
        if m[pivot][col].is_zero() {
            return None; // singular
        }
        m.swap(col, pivot);
        b.swap(col, pivot);

        let pv = m[col][col];
        for x in &mut m[col][col..] {
            *x /= pv;
        }
        b[col] /= pv;

        // Snapshot the normalized pivot row so we can mutate the other rows
        // without aliasing `m[col]`.
        let pivot_row = m[col][col..].to_vec();
        let pivot_rhs = b[col];

        // Eliminate every other row.
        for row in 0..n {
            if row == col {
                continue;
            }
            let factor = m[row][col];
            if factor.is_zero() {
                continue;
            }
            for (j, &pivot) in pivot_row.iter().enumerate() {
                m[row][col + j] -= factor * pivot;
            }
            b[row] -= factor * pivot_rhs;
        }
    }
    Some(b)
}
