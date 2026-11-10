//! Phase 3 — account-vs-account / account-vs-prior-period comparison.
//!
//! Runs the Phase 2 metrics engine over two `(account, date_range)` pairs and
//! returns both results side by side for a UI to diff (Vela's Compare screen:
//! "Two performances, same metrics," with a `vs account` / `vs previous`
//! toggle).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::metrics::{EdgeScore, PerformanceMetrics, compute_edge_score, compute_metrics};
use crate::model::ClosedTrade;

/// One side of a comparison: the metrics for a single account over a range.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountMetricsView {
    pub account_id: crate::model::JournalAccountId,
    pub range_from: Option<chrono::NaiveDate>,
    pub range_to: Option<chrono::NaiveDate>,
    pub metrics: PerformanceMetrics,
    pub edge_score: EdgeScore,
}

/// A two-sided comparison of two accounts (or one account vs. its own prior
/// period).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountComparison {
    pub account_a: AccountMetricsView,
    pub account_b: AccountMetricsView,
    /// `true` when `account_b` represents the same account's prior period.
    pub is_prior_period: bool,
}

/// Build a comparison of two accounts over their respective closed-trade
/// streams. `as_of` is injected by the caller (temporal determinism).
pub fn compare_accounts(
    account_a_id: crate::model::JournalAccountId,
    trades_a: Vec<ClosedTrade>,
    account_b_id: crate::model::JournalAccountId,
    trades_b: Vec<ClosedTrade>,
    as_of: DateTime<Utc>,
    is_prior_period: bool,
) -> AccountComparison {
    let metrics_a = compute_metrics(&crate::metrics::MetricsInput::new(trades_a).with_as_of(as_of))
        .unwrap_or_default_metrics();
    let edge_a = compute_edge_score(&metrics_a);

    let metrics_b = compute_metrics(&crate::metrics::MetricsInput::new(trades_b).with_as_of(as_of))
        .unwrap_or_default_metrics();
    let edge_b = compute_edge_score(&metrics_b);

    AccountComparison {
        account_a: AccountMetricsView {
            account_id: account_a_id,
            range_from: None,
            range_to: None,
            metrics: metrics_a,
            edge_score: edge_a,
        },
        account_b: AccountMetricsView {
            account_id: account_b_id,
            range_from: None,
            range_to: None,
            metrics: metrics_b,
            edge_score: edge_b,
        },
        is_prior_period,
    }
}

/// Metrics for one account vs. its own prior period: splits the account's
/// closed trades into current and prior periods at `boundary` (exclusive for
/// the prior side) and compares them.
pub fn compare_account_vs_prior(
    account_id: crate::model::JournalAccountId,
    trades: Vec<ClosedTrade>,
    boundary: chrono::NaiveDate,
    as_of: DateTime<Utc>,
) -> AccountComparison {
    let (prior, current): (Vec<ClosedTrade>, Vec<ClosedTrade>) =
        trades.into_iter().partition(|t| t.exit_date < boundary);

    let metrics_current =
        compute_metrics(&crate::metrics::MetricsInput::new(current).with_as_of(as_of))
            .unwrap_or_default_metrics();
    let edge_current = compute_edge_score(&metrics_current);

    let metrics_prior =
        compute_metrics(&crate::metrics::MetricsInput::new(prior).with_as_of(as_of))
            .unwrap_or_default_metrics();
    let edge_prior = compute_edge_score(&metrics_prior);

    AccountComparison {
        account_a: AccountMetricsView {
            account_id: account_id.clone(),
            range_from: None,
            range_to: None,
            metrics: metrics_current,
            edge_score: edge_current,
        },
        account_b: AccountMetricsView {
            account_id,
            range_from: None,
            range_to: None,
            metrics: metrics_prior,
            edge_score: edge_prior,
        },
        is_prior_period: true,
    }
}

// Default metrics for when compute_metrics errors (e.g. empty trade list).
trait DefaultMetrics {
    fn unwrap_or_default_metrics(self) -> crate::metrics::PerformanceMetrics;
}

impl DefaultMetrics for Result<crate::metrics::PerformanceMetrics, crate::metrics::MetricsError> {
    fn unwrap_or_default_metrics(self) -> crate::metrics::PerformanceMetrics {
        match self {
            Ok(m) => m,
            Err(_) => crate::metrics::PerformanceMetrics {
                total_trades: 0,
                closed_trades: 0,
                open_trades: 0,
                wins: 0,
                losses: 0,
                breakevens: 0,
                net_pnl: crate::Money::zero_inr(),
                gross_pnl: crate::Money::zero_inr(),
                fees: crate::Money::zero_inr(),
                win_rate: None,
                day_win_rate: None,
                trading_days: 0,
                profit_factor: None,
                profit_factor_is_infinite: false,
                avg_win: None,
                avg_loss: None,
                avg_win_loss_ratio: None,
                expectancy: None,
                largest_win: crate::Money::zero_inr(),
                largest_loss: crate::Money::zero_inr(),
                max_win_streak: 0,
                max_loss_streak: 0,
                current_streak: 0,
                total_volume: crate::Money::zero_inr(),
                avg_duration_ms: None,
                max_drawdown: crate::Money::zero_inr(),
                max_drawdown_pct: None,
                recovery_factor: None,
                profit_concentration: None,
                avg_realized_r: None,
                trades_with_risk: 0,
            },
        }
    }
}
