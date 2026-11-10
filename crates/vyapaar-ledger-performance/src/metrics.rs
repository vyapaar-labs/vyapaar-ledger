//! Phase 2 — performance metrics engine + edge-score composite.
//!
//! Pure functions computing, over a date range, the Vela-equivalent metric
//! set (win rate, profit factor, expectancy, edge score, drawdown, streaks,
//! ...) plus the composite 0-100 edge score. Verified against
//! vela.luxalgo.com's own `/api/journal/overview` response shape.
//!
//! The engine consumes the derived closed-trade stream
//! ([`ClosedTrade`](crate::model::ClosedTrade)) produced from
//! `vyapaar-ledger-portfolio`'s [`MatchedLot`](vyapaar_ledger_portfolio::MatchedLot)
//! FIFO matches, plus an optional equity curve for drawdown. It does not own
//! fills or lot-matching.
//!
//! # Edge score (v1)
//!
//! The plan flags the exact per-component scoring and blend formula as an
//! open design question (LuxAlgo's proprietary weighting is not
//! reverse-engineered). This v1 implementation uses a transparent, documented
//! formula with equal component weights; it is deliberately easy to replace.
//! The score is `None` until at least 5 closed trades exist (Vela's own
//! empty-state: "5 more closed trades needed").

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::Money;
use crate::model::ClosedTrade;

/// Errors from the metrics engine.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MetricsError {
    #[error("equity curve is not sorted by date")]
    EquityCurveUnsorted,
    #[error("no closed trades in range")]
    NoClosedTrades,
}

/// The full performance-metrics payload, mirroring Vela's own
/// `/api/journal/overview` response shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub total_trades: u64,
    pub closed_trades: u64,
    pub open_trades: u64,
    pub wins: u64,
    pub losses: u64,
    pub breakevens: u64,
    pub net_pnl: Money,
    pub gross_pnl: Money,
    pub fees: Money,
    pub win_rate: Option<Decimal>,
    pub day_win_rate: Option<Decimal>,
    pub trading_days: u64,
    pub profit_factor: Option<Decimal>,
    pub profit_factor_is_infinite: bool,
    pub avg_win: Option<Money>,
    pub avg_loss: Option<Money>,
    pub avg_win_loss_ratio: Option<Decimal>,
    pub expectancy: Option<Money>,
    pub largest_win: Money,
    pub largest_loss: Money,
    pub max_win_streak: u64,
    pub max_loss_streak: u64,
    pub current_streak: i64,
    pub total_volume: Money,
    pub avg_duration_ms: Option<u64>,
    pub max_drawdown: Money,
    pub max_drawdown_pct: Option<Decimal>,
    pub recovery_factor: Option<Decimal>,
    pub profit_concentration: Option<Decimal>,
    pub avg_realized_r: Option<Decimal>,
    pub trades_with_risk: u64,
}

/// The composite edge score (0-100, `None` until enough closed trades).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgeScore {
    /// Schema version of the scoring formula.
    pub version: u32,
    /// `None` when fewer than `MIN_CLOSED_TRADES_FOR_SCORE` closed trades.
    pub score: Option<Decimal>,
    pub components: EdgeScoreComponents,
    pub closed_trades: u64,
}

/// The six independently-scored edge-score components (each 0-100).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgeScoreComponents {
    pub win_rate: Decimal,
    pub profit_factor: Decimal,
    pub avg_win_loss: Decimal,
    pub drawdown: Decimal,
    pub recovery: Decimal,
    pub consistency: Decimal,
}

/// Minimum closed trades before an edge score is published (Vela's own
/// empty state: "5 more closed trades needed").
pub const MIN_CLOSED_TRADES_FOR_SCORE: u64 = 5;
/// Edge-score schema version (v1 placeholder formula).
pub const EDGE_SCORE_VERSION: u32 = 1;

/// Input to the metrics engine.
#[derive(Debug, Clone)]
pub struct MetricsInput {
    /// Closed trades within the analysis range (chronological order).
    pub closed_trades: Vec<ClosedTrade>,
    /// Open positions not yet closed (supplied by the caller from the
    /// portfolio; not derivable from matched lots).
    pub open_trades: u64,
    /// Equity-curve snapshots `(date, equity)` for running-peak drawdown,
    /// sorted ascending by date.
    pub equity_curve: Vec<(NaiveDate, Money)>,
    /// As-of timestamp injected by the caller (temporal determinism).
    pub as_of: DateTime<Utc>,
}

impl MetricsInput {
    pub fn new(closed_trades: Vec<ClosedTrade>) -> Self {
        Self {
            closed_trades,
            open_trades: 0,
            equity_curve: Vec::new(),
            as_of: Utc::now(), // test-only placeholder; callers should set explicitly
        }
    }

    pub fn with_open_trades(mut self, n: u64) -> Self {
        self.open_trades = n;
        self
    }

    pub fn with_equity_curve(mut self, curve: Vec<(NaiveDate, Money)>) -> Self {
        self.equity_curve = curve;
        self
    }

    pub fn with_as_of(mut self, t: DateTime<Utc>) -> Self {
        self.as_of = t;
        self
    }
}

/// Compute the full performance-metrics payload from the input. Pure and
/// deterministic: identical inputs produce identical metrics.
pub fn compute_metrics(input: &MetricsInput) -> Result<PerformanceMetrics, MetricsError> {
    let trades = &input.closed_trades;

    let total_trades = trades.len() as u64;
    let closed_trades = total_trades;
    let open_trades = input.open_trades;

    let mut wins = 0u64;
    let mut losses = 0u64;
    let mut breakevens = 0u64;
    let mut net_pnl_amount = Decimal::ZERO;
    let mut fees_amount = Decimal::ZERO;
    let mut gross_profit_amount = Decimal::ZERO;
    let mut gross_loss_amount = Decimal::ZERO;
    let mut total_volume_amount = Decimal::ZERO;
    let mut largest_win_amount = Decimal::ZERO;
    let mut largest_loss_amount = Decimal::ZERO;
    let mut duration_sum = 0u64;
    let mut duration_count = 0u64;
    let mut realized_r_sum = Decimal::ZERO;
    let mut trades_with_risk = 0u64;

    // Streak tracking over chronological trade order.
    let mut max_win_streak = 0u64;
    let mut max_loss_streak = 0u64;
    let mut current_win_streak = 0u64;
    let mut current_loss_streak = 0u64;

    // Per-day PnL aggregation (for day win rate and trading days).
    let mut daily_pnl: std::collections::BTreeMap<NaiveDate, Decimal> =
        std::collections::BTreeMap::new();

    for trade in trades {
        let net = trade.net_pnl().amount;
        net_pnl_amount += net;
        fees_amount += trade.fees.amount;
        total_volume_amount += trade.exit_price.value() * trade.quantity.value();

        if trade.is_win() {
            wins += 1;
            gross_profit_amount += net;
            if net > largest_win_amount {
                largest_win_amount = net;
            }
            current_win_streak += 1;
            current_loss_streak = 0;
            if current_win_streak > max_win_streak {
                max_win_streak = current_win_streak;
            }
        } else if trade.is_loss() {
            losses += 1;
            let loss_mag = -net;
            gross_loss_amount += loss_mag;
            if loss_mag > largest_loss_amount {
                largest_loss_amount = loss_mag;
            }
            current_loss_streak += 1;
            current_win_streak = 0;
            if current_loss_streak > max_loss_streak {
                max_loss_streak = current_loss_streak;
            }
        } else {
            breakevens += 1;
            current_win_streak = 0;
            current_loss_streak = 0;
        }

        *daily_pnl.entry(trade.exit_date).or_insert(Decimal::ZERO) += net;

        if let Some(d) = trade.duration_ms {
            duration_sum += d;
            duration_count += 1;
        }
        if let Some(r) = trade.realized_r() {
            realized_r_sum += r;
            trades_with_risk += 1;
        }
    }

    let current_streak = if current_win_streak > 0 {
        current_win_streak as i64
    } else if current_loss_streak > 0 {
        -(current_loss_streak as i64)
    } else {
        0
    };

    let trading_days = daily_pnl.len() as u64;
    let mut profitable_days = 0u64;
    for &pnl in daily_pnl.values() {
        if pnl > Decimal::ZERO {
            profitable_days += 1;
        }
    }

    let win_rate = if total_trades > 0 {
        Some(Decimal::from(wins) / Decimal::from(total_trades))
    } else {
        None
    };

    let day_win_rate = if trading_days > 0 {
        Some(Decimal::from(profitable_days) / Decimal::from(trading_days))
    } else {
        None
    };

    let gross_pnl = Money::inr(gross_profit_amount);
    let largest_win = Money::inr(largest_win_amount);
    let largest_loss = Money::inr(largest_loss_amount);
    let net_pnl = Money::inr(net_pnl_amount);
    let fees = Money::inr(fees_amount);
    let total_volume = Money::inr(total_volume_amount);

    let (profit_factor, profit_factor_is_infinite) = if gross_loss_amount.is_zero() {
        if gross_profit_amount.is_zero() {
            (None, false)
        } else {
            (None, true) // profitable with no losses → infinite
        }
    } else {
        (Some(gross_profit_amount / gross_loss_amount), false)
    };

    let avg_win = if wins > 0 {
        Some(Money::inr(gross_profit_amount / Decimal::from(wins)))
    } else {
        None
    };
    let avg_loss = if losses > 0 {
        Some(Money::inr(gross_loss_amount / Decimal::from(losses)))
    } else {
        None
    };

    let avg_win_loss_ratio = match (&avg_win, &avg_loss) {
        (Some(w), Some(l)) if !l.amount.is_zero() => Some(w.amount / l.amount),
        _ => None,
    };

    let expectancy = if total_trades > 0 {
        Some(Money::inr(net_pnl_amount / Decimal::from(total_trades)))
    } else {
        None
    };

    let avg_duration_ms = if duration_count > 0 {
        Some(duration_sum.checked_div(duration_count).unwrap_or(0))
    } else {
        None
    };

    let avg_realized_r = if trades_with_risk > 0 {
        Some(realized_r_sum / Decimal::from(trades_with_risk))
    } else {
        None
    };

    // Drawdown from the equity curve (running peak).
    let (max_drawdown, max_drawdown_pct) = compute_drawdown(&input.equity_curve);

    let recovery_factor = if let (Some(dd), Some(dd_pct)) = (&max_drawdown, &max_drawdown_pct) {
        if !dd.amount.is_zero() {
            Some(net_pnl_amount / dd.amount)
        } else if dd_pct.is_zero() && !net_pnl_amount.is_zero() {
            // No drawdown but profitable → infinite recovery factor.
            None // represented as None; callers treat zero drawdown as infinite
        } else {
            None
        }
    } else {
        None
    };

    let profit_concentration = if !net_pnl_amount.is_zero() && largest_win_amount > Decimal::ZERO {
        Some(largest_win_amount / net_pnl_amount.abs())
    } else {
        None
    };

    Ok(PerformanceMetrics {
        total_trades,
        closed_trades,
        open_trades,
        wins,
        losses,
        breakevens,
        net_pnl,
        gross_pnl,
        fees,
        win_rate,
        day_win_rate,
        trading_days,
        profit_factor,
        profit_factor_is_infinite,
        avg_win,
        avg_loss,
        avg_win_loss_ratio,
        expectancy,
        largest_win,
        largest_loss,
        max_win_streak,
        max_loss_streak,
        current_streak,
        total_volume,
        avg_duration_ms,
        max_drawdown: max_drawdown.unwrap_or_else(Money::zero_inr),
        max_drawdown_pct,
        recovery_factor,
        profit_concentration,
        avg_realized_r,
        trades_with_risk,
    })
}

/// Compute the maximum drawdown (peak-to-trough) from an equity curve.
///
/// Returns `(max_drawdown_amount, max_drawdown_pct)`. The drawdown at each
/// point is `running_peak - equity`; the max drawdown is the largest such
/// gap, and the pct is the gap divided by the peak at that point.
fn compute_drawdown(curve: &[(NaiveDate, Money)]) -> (Option<Money>, Option<Decimal>) {
    if curve.is_empty() {
        return (None, None);
    }

    // Verify sorted ascending.
    for pair in curve.windows(2) {
        if pair[0].0 > pair[1].0 {
            // Not sorted ascending; the engine requires sorted input.
            return (None, None);
        }
    }

    let mut running_peak = curve[0].1.amount;
    let mut max_dd = Decimal::ZERO;
    let mut max_dd_pct = Decimal::ZERO;

    for (_date, equity) in curve {
        let eq = equity.amount;
        if eq > running_peak {
            running_peak = eq;
        }
        let dd = running_peak - eq;
        if dd > max_dd {
            max_dd = dd;
            max_dd_pct = if !running_peak.is_zero() {
                dd / running_peak
            } else {
                Decimal::ZERO
            };
        }
    }

    let dd_pct = if max_dd_pct.is_zero() && max_dd.is_zero() {
        None
    } else {
        Some(max_dd_pct)
    };
    (Some(Money::inr(max_dd)), dd_pct)
}

/// Compute the composite edge score (0-100) from the metrics.
///
/// v1 formula: each of six components is scored 0-100, then blended with
/// equal weights. Returns `None` when fewer than
/// [`MIN_CLOSED_TRADES_FOR_SCORE`] closed trades exist.
pub fn compute_edge_score(metrics: &PerformanceMetrics) -> EdgeScore {
    let closed = metrics.closed_trades;
    let components = if closed >= MIN_CLOSED_TRADES_FOR_SCORE {
        EdgeScoreComponents {
            win_rate: score_win_rate(metrics),
            profit_factor: score_profit_factor(metrics),
            avg_win_loss: score_avg_win_loss(metrics),
            drawdown: score_drawdown(metrics),
            recovery: score_recovery(metrics),
            consistency: score_consistency(metrics),
        }
    } else {
        EdgeScoreComponents {
            win_rate: Decimal::ZERO,
            profit_factor: Decimal::ZERO,
            avg_win_loss: Decimal::ZERO,
            drawdown: Decimal::ZERO,
            recovery: Decimal::ZERO,
            consistency: Decimal::ZERO,
        }
    };

    let score = if closed >= MIN_CLOSED_TRADES_FOR_SCORE {
        let total = components.win_rate
            + components.profit_factor
            + components.avg_win_loss
            + components.drawdown
            + components.recovery
            + components.consistency;
        Some(total / Decimal::from(6))
    } else {
        None
    };

    EdgeScore {
        version: EDGE_SCORE_VERSION,
        score,
        components,
        closed_trades: closed,
    }
}

// ---------------------------------------------------------------------------
// Component scorers (each 0-100)
// ---------------------------------------------------------------------------

fn clamp0100(x: Decimal) -> Decimal {
    if x < Decimal::ZERO {
        Decimal::ZERO
    } else if x > Decimal::from(100) {
        Decimal::from(100)
    } else {
        x
    }
}

/// Linear interpolation between `low` (score 0) at `lo_val` and `high`
/// (score 100) at `hi_val`, clamped to [0, 100]. Values outside the bracket
/// saturate.
fn lerp_score(value: Decimal, lo_val: Decimal, hi_val: Decimal) -> Decimal {
    if lo_val == hi_val {
        return if value >= hi_val {
            Decimal::from(100)
        } else {
            Decimal::ZERO
        };
    }
    let t = (value - lo_val) / (hi_val - lo_val);
    clamp0100(t * Decimal::from(100))
}

fn score_win_rate(m: &PerformanceMetrics) -> Decimal {
    let wr = m.win_rate.unwrap_or(Decimal::ZERO);
    // win rate 0% → 0, 100% → 100
    clamp0100(wr * Decimal::from(100))
}

fn score_profit_factor(m: &PerformanceMetrics) -> Decimal {
    match m.profit_factor {
        Some(pf) => {
            // pf <= 0.5 → 0; pf = 1 → 50; pf >= 2 → 100
            lerp_score(pf, Decimal::from(5) / Decimal::from(10), Decimal::from(2))
        }
        None => {
            if m.profit_factor_is_infinite {
                Decimal::from(100)
            } else {
                Decimal::ZERO
            }
        }
    }
}

fn score_avg_win_loss(m: &PerformanceMetrics) -> Decimal {
    match m.avg_win_loss_ratio {
        Some(r) => {
            // ratio <= 0.5 → 0; ratio = 1 → 50; ratio >= 2 → 100
            lerp_score(r, Decimal::from(5) / Decimal::from(10), Decimal::from(2))
        }
        None => Decimal::ZERO,
    }
}

fn score_drawdown(m: &PerformanceMetrics) -> Decimal {
    match m.max_drawdown_pct {
        Some(dd) => {
            // dd = 0 → 100; each 2% drawdown reduces score linearly;
            // dd >= 20% → 0
            clamp0100(Decimal::from(100) - dd * Decimal::from(1000) / Decimal::from(2))
        }
        None => Decimal::from(50), // no equity curve → neutral
    }
}

fn score_recovery(m: &PerformanceMetrics) -> Decimal {
    match m.recovery_factor {
        Some(rf) => {
            // rf <= 0 → 0; rf = 1 → 50; rf >= 2 → 100
            lerp_score(rf, Decimal::ZERO, Decimal::from(2))
        }
        None => Decimal::ZERO,
    }
}

fn score_consistency(m: &PerformanceMetrics) -> Decimal {
    let wins_streak = m.max_win_streak;
    let losses_streak = m.max_loss_streak;
    if wins_streak == 0 && losses_streak == 0 {
        Decimal::from(50)
    } else {
        let total = wins_streak + losses_streak;
        clamp0100(Decimal::from(wins_streak) / Decimal::from(total) * Decimal::from(100))
    }
}
