//! Golden-vector and error-path tests for `vyapaar-ledger-allocation`.

use chrono::{DateTime, TimeZone, Utc};
use rust_decimal::Decimal;
use std::collections::BTreeMap;
use vyapaar_core::{Currency, Exchange, InstrumentId, Price, Quantity};
use vyapaar_ledger_allocation::{
    AllocationError, AllocationObjective, Constraint, PortfolioState, TargetWeights, optimize,
};
use vyapaar_ledger_portfolio::{AccountState, HeldPosition, Marks, PositionSide};

fn d(s: &str) -> Decimal {
    s.parse().unwrap()
}

fn instrument(symbol: &str) -> InstrumentId {
    InstrumentId::new(Exchange::Nse, symbol)
}

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).single().unwrap()
}

/// Two-asset state: A has return 0.10, B 0.12, with the golden covariance
/// `[[1, 0.5], [0.5, 1]]`.
fn two_asset_state() -> PortfolioState {
    let mut expected_returns = BTreeMap::new();
    expected_returns.insert(instrument("A"), d("0.10"));
    expected_returns.insert(instrument("B"), d("0.12"));

    let mut covariance = BTreeMap::new();
    let mut row_a = BTreeMap::new();
    row_a.insert(instrument("A"), d("1"));
    row_a.insert(instrument("B"), d("0.5"));
    let mut row_b = BTreeMap::new();
    row_b.insert(instrument("A"), d("0.5"));
    row_b.insert(instrument("B"), d("1"));
    covariance.insert(instrument("A"), row_a);
    covariance.insert(instrument("B"), row_b);

    PortfolioState {
        as_of: now(),
        instruments: vec![instrument("A"), instrument("B")],
        current_weights: BTreeMap::new(),
        expected_returns,
        covariance,
    }
}

/// Diagonal covariance `diag(0.04, 0.16)` → volatilities 0.2 and 0.4, so risk
/// parity targets inverse-vol weights `[2/3, 1/3]`.
fn diagonal_covariance_state() -> PortfolioState {
    let mut covariance = BTreeMap::new();
    let mut row_a = BTreeMap::new();
    row_a.insert(instrument("A"), d("0.04"));
    row_a.insert(instrument("B"), d("0"));
    let mut row_b = BTreeMap::new();
    row_b.insert(instrument("A"), d("0"));
    row_b.insert(instrument("B"), d("0.16"));
    covariance.insert(instrument("A"), row_a);
    covariance.insert(instrument("B"), row_b);

    PortfolioState {
        as_of: now(),
        instruments: vec![instrument("A"), instrument("B")],
        current_weights: BTreeMap::new(),
        expected_returns: BTreeMap::new(),
        covariance,
    }
}

fn weights(target: &TargetWeights) -> BTreeMap<String, Decimal> {
    target
        .weights
        .iter()
        .map(|(id, w)| (id.symbol.clone(), *w))
        .collect()
}

/// `&str` → `String`, for indexing the `BTreeMap<String, Decimal>` from
/// [`weights`] without inference ambiguity.
fn sym(s: &str) -> String {
    s.to_string()
}

fn assert_sums_to_one(target: &TargetWeights) {
    let sum: Decimal = target.weights.values().copied().sum();
    assert!(
        (sum - Decimal::ONE).abs() < d("0.000001"),
        "weights sum to {sum}, expected 1"
    );
}

// ---------------------------------------------------------------------------
// Mean-Variance
// ---------------------------------------------------------------------------

#[test]
fn mean_variance_tangency_golden() {
    let objective = AllocationObjective::MeanVariance {
        risk_free_rate: d("0.05"),
    };
    let target = optimize(objective, &[], &two_asset_state()).unwrap();

    // Σ⁻¹(μ − r_f) = [0.02, 0.06], normalized to [0.25, 0.75].
    let w = weights(&target);
    assert_eq!(w[&sym("A")], d("0.25"));
    assert_eq!(w[&sym("B")], d("0.75"));
    assert_sums_to_one(&target);
    assert_eq!(target.as_of, now());
}

#[test]
fn mean_variance_respects_weight_bounds() {
    let objective = AllocationObjective::MeanVariance {
        risk_free_rate: d("0.05"),
    };
    let constraints = [Constraint::WeightBounds {
        instrument: instrument("B"),
        min: d("0"),
        max: d("0.5"),
    }];
    let target = optimize(objective, &constraints, &two_asset_state()).unwrap();

    // Tangency [0.25, 0.75] clips B to 0.5; A renormalizes to 0.5.
    let w = weights(&target);
    assert_eq!(w[&sym("A")], d("0.5"));
    assert_eq!(w[&sym("B")], d("0.5"));
    assert_sums_to_one(&target);
}

#[test]
fn mean_variance_requires_expected_returns() {
    let mut state = two_asset_state();
    state.expected_returns = BTreeMap::new();
    let err = optimize(
        AllocationObjective::MeanVariance {
            risk_free_rate: d("0.05"),
        },
        &[],
        &state,
    )
    .unwrap_err();
    assert!(
        matches!(err, AllocationError::InvalidArgument(msg) if msg.contains("missing expected return"))
    );
}

#[test]
fn mean_variance_singular_covariance_is_an_error() {
    let mut state = two_asset_state();
    let mut covariance = BTreeMap::new();
    let mut row_a = BTreeMap::new();
    row_a.insert(instrument("A"), d("1"));
    row_a.insert(instrument("B"), d("2"));
    let mut row_b = BTreeMap::new();
    row_b.insert(instrument("A"), d("2"));
    row_b.insert(instrument("B"), d("4"));
    covariance.insert(instrument("A"), row_a);
    covariance.insert(instrument("B"), row_b);
    state.covariance = covariance;

    assert_eq!(
        optimize(
            AllocationObjective::MeanVariance {
                risk_free_rate: d("0.05"),
            },
            &[],
            &state,
        )
        .unwrap_err(),
        AllocationError::SingularCovariance
    );
}

// ---------------------------------------------------------------------------
// Risk Parity
// ---------------------------------------------------------------------------

#[test]
fn risk_parity_diagonal_covariance_is_inverse_volatility() {
    let target = optimize(
        AllocationObjective::RiskParity,
        &[],
        &diagonal_covariance_state(),
    )
    .unwrap();

    // w ∝ [1/0.2, 1/0.4] = [5, 2.5] → [2/3, 1/3].
    let w = weights(&target);
    assert_eq!(w[&sym("A")].round_dp(6), d("0.666667"));
    assert_eq!(w[&sym("B")].round_dp(6), d("0.333333"));
    assert_sums_to_one(&target);
}

#[test]
fn risk_parity_respects_weight_bounds() {
    let constraints = [Constraint::WeightBounds {
        instrument: instrument("A"),
        min: d("0"),
        max: d("0.5"),
    }];
    let target = optimize(
        AllocationObjective::RiskParity,
        &constraints,
        &diagonal_covariance_state(),
    )
    .unwrap();
    let w = weights(&target);
    assert!(w[&sym("A")] <= d("0.5"));
    assert_sums_to_one(&target);
}

#[test]
fn risk_parity_singular_covariance_is_an_error() {
    // PSD singular matrix: under equal weights the marginal risk vector is
    // all zeros, so the risk-parity reciprocal blows up.
    let mut state = diagonal_covariance_state();
    state.covariance = {
        let mut covariance = BTreeMap::new();
        let mut row_a = BTreeMap::new();
        row_a.insert(instrument("A"), d("1"));
        row_a.insert(instrument("B"), d("-1"));
        let mut row_b = BTreeMap::new();
        row_b.insert(instrument("A"), d("-1"));
        row_b.insert(instrument("B"), d("1"));
        covariance.insert(instrument("A"), row_a);
        covariance.insert(instrument("B"), row_b);
        covariance
    };
    assert!(matches!(
        optimize(AllocationObjective::RiskParity, &[], &state),
        Err(AllocationError::SingularCovariance)
    ));
}

// ---------------------------------------------------------------------------
// Constraint validation
// ---------------------------------------------------------------------------

#[test]
fn min_greater_than_max_is_infeasible() {
    let constraints = [Constraint::WeightBounds {
        instrument: instrument("A"),
        min: d("0.6"),
        max: d("0.5"),
    }];
    let err = optimize(
        AllocationObjective::RiskParity,
        &constraints,
        &diagonal_covariance_state(),
    )
    .unwrap_err();
    assert!(
        matches!(err, AllocationError::InfeasibleConstraints(msg) if msg.contains("min 0.6 > max 0.5"))
    );
}

#[test]
fn sum_of_mins_above_one_is_infeasible() {
    let constraints = [
        Constraint::WeightBounds {
            instrument: instrument("A"),
            min: d("0.6"),
            max: d("1"),
        },
        Constraint::WeightBounds {
            instrument: instrument("B"),
            min: d("0.6"),
            max: d("1"),
        },
    ];
    let err = optimize(
        AllocationObjective::RiskParity,
        &constraints,
        &diagonal_covariance_state(),
    )
    .unwrap_err();
    assert!(
        matches!(err, AllocationError::InfeasibleConstraints(msg) if msg.contains("sum of minimum weight bounds exceeds 1"))
    );
}

#[test]
fn sum_of_maxes_below_one_is_infeasible() {
    let constraints = [
        Constraint::WeightBounds {
            instrument: instrument("A"),
            min: d("0"),
            max: d("0.4"),
        },
        Constraint::WeightBounds {
            instrument: instrument("B"),
            min: d("0"),
            max: d("0.4"),
        },
    ];
    let err = optimize(
        AllocationObjective::RiskParity,
        &constraints,
        &diagonal_covariance_state(),
    )
    .unwrap_err();
    assert!(
        matches!(err, AllocationError::InfeasibleConstraints(msg) if msg.contains("sum of maximum weight bounds is below 1"))
    );
}

#[test]
fn constraint_for_unknown_instrument_is_an_error() {
    let constraints = [Constraint::WeightBounds {
        instrument: instrument("C"),
        min: d("0"),
        max: d("1"),
    }];
    let err = optimize(
        AllocationObjective::RiskParity,
        &constraints,
        &diagonal_covariance_state(),
    )
    .unwrap_err();
    assert!(
        matches!(err, AllocationError::InvalidArgument(msg) if msg.contains("not in the universe"))
    );
}

// ---------------------------------------------------------------------------
// Not-yet-implemented objectives and input validation
// ---------------------------------------------------------------------------

#[test]
fn hrp_and_black_litterman_are_not_implemented() {
    let state = two_asset_state();
    assert!(matches!(
        optimize(AllocationObjective::Hrp, &[], &state),
        Err(AllocationError::NotImplemented(msg)) if msg.contains("Phase 2")
    ));
    assert!(matches!(
        optimize(AllocationObjective::BlackLitterman, &[], &state),
        Err(AllocationError::NotImplemented(msg)) if msg.contains("Phase 3")
    ));
}

#[test]
fn empty_universe_is_an_error() {
    let state = PortfolioState {
        as_of: now(),
        instruments: vec![],
        current_weights: BTreeMap::new(),
        expected_returns: BTreeMap::new(),
        covariance: BTreeMap::new(),
    };
    assert_eq!(
        optimize(AllocationObjective::RiskParity, &[], &state).unwrap_err(),
        AllocationError::InvalidArgument("instrument universe is empty".into())
    );
}

#[test]
fn missing_covariance_row_is_an_error() {
    let mut state = two_asset_state();
    state.covariance = BTreeMap::new();
    let err = optimize(AllocationObjective::RiskParity, &[], &state).unwrap_err();
    assert!(
        matches!(err, AllocationError::InvalidArgument(msg) if msg.contains("missing covariance row"))
    );
}

// ---------------------------------------------------------------------------
// Read-adapter over vyapaar-ledger-portfolio
// ---------------------------------------------------------------------------

#[test]
fn from_portfolio_computes_current_weights_and_optimizes() {
    let instr_a = instrument("A");
    let instr_b = instrument("B");

    let mut portfolio = vyapaar_ledger_portfolio::PortfolioState::new(Currency::Inr);
    let account = AccountState::new(
        Currency::Inr,
        vec![
            HeldPosition::new(
                instr_a.clone(),
                PositionSide::Long,
                Quantity::from_u64(100),
                Price::from_rupees("10").unwrap(),
            ),
            HeldPosition::new(
                instr_b.clone(),
                PositionSide::Long,
                Quantity::from_u64(30),
                Price::from_rupees("10").unwrap(),
            ),
        ],
    );
    portfolio.accounts.insert("acc-1".into(), account);

    let marks = Marks::new()
        .with(instr_a.clone(), Price::from_rupees("10").unwrap())
        .with(instr_b.clone(), Price::from_rupees("10").unwrap());

    let state = PortfolioState::from_portfolio(&portfolio, &marks, now());
    assert_eq!(state.instruments.len(), 2);
    assert_eq!(
        state.current_weights[&instr_a].round_dp(6),
        d("0.769231") // 1000 / 1300
    );
    assert_eq!(
        state.current_weights[&instr_b].round_dp(6),
        d("0.230769") // 300 / 1300
    );

    // Attach the same golden inputs as the other tests and optimize.
    let mut expected_returns = BTreeMap::new();
    expected_returns.insert(instr_a.clone(), d("0.10"));
    expected_returns.insert(instr_b.clone(), d("0.12"));
    let mut covariance = BTreeMap::new();
    let mut row_a = BTreeMap::new();
    row_a.insert(instr_a.clone(), d("1"));
    row_a.insert(instr_b.clone(), d("0.5"));
    let mut row_b = BTreeMap::new();
    row_b.insert(instr_a.clone(), d("0.5"));
    row_b.insert(instr_b.clone(), d("1"));
    covariance.insert(instr_a.clone(), row_a);
    covariance.insert(instr_b.clone(), row_b);

    let state = state
        .with_expected_returns(expected_returns)
        .with_covariance(covariance);
    let target = optimize(
        AllocationObjective::MeanVariance {
            risk_free_rate: d("0.05"),
        },
        &[],
        &state,
    )
    .unwrap();
    assert_eq!(target.weights[&instr_a], d("0.25"));
    assert_eq!(target.weights[&instr_b], d("0.75"));
}

// ---------------------------------------------------------------------------
// Serde / auditability
// ---------------------------------------------------------------------------

#[test]
fn target_weights_serde_roundtrip() {
    let target = optimize(
        AllocationObjective::RiskParity,
        &[],
        &diagonal_covariance_state(),
    )
    .unwrap();
    let json = serde_json::to_string(&target).unwrap();
    let back: TargetWeights = serde_json::from_str(&json).unwrap();
    assert_eq!(target, back);
    assert_eq!(back.objective, AllocationObjective::RiskParity);
}

#[test]
fn objective_and_constraint_serde_roundtrip() {
    let objective = AllocationObjective::MeanVariance {
        risk_free_rate: d("0.05"),
    };
    let json = serde_json::to_string(&objective).unwrap();
    let back: AllocationObjective = serde_json::from_str(&json).unwrap();
    assert_eq!(objective, back);

    let constraint = Constraint::WeightBounds {
        instrument: instrument("A"),
        min: d("0"),
        max: d("1"),
    };
    let json = serde_json::to_string(&constraint).unwrap();
    let back: Constraint = serde_json::from_str(&json).unwrap();
    assert_eq!(constraint, back);
}
