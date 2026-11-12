# `vyapaar-ledger-performance` — Trade Performance Journal Reference

Documents `crates/vyapaar-ledger-performance/` (`src/lib.rs` plus the modules
`model`, `store`, `metrics`, `calendar`, `compare`): the trader's
performance/annotations journal — a **thin layer on top of
`vyapaar-ledger-portfolio`**, not a re-implementation of FIFO lot matching.

> Naming: this crate deliberately avoids the word "journal" in its name —
> `vyapaar-ledger-journal` already owns the double-entry accounting meaning.

## Layering

`vyapaar-ledger-portfolio::AccountState::match_lots` derives FIFO round-trips as
`MatchedLot`s — each a closed trade leg with realized PnL. This crate consumes
that closed-trade stream (via `ClosedTrade::from_matched_lot`), attaches user
annotations, and computes analytics. It does not own fills or lot-matching.

## Phase 1 — data model & journal store

`src/model.rs`:

| Type | Purpose |
|------|---------|
| `JournalAccountId` | Analytics grouping identifier, distinct from the live OMS/portfolio `AccountId`. |
| `AccountProvenance` | `Manual` / `Imported` / `BrokerSynced`. |
| `PnlConvention` | v1 hard-codes `Fifo`; the enum is modeled for a future convention. |
| `JournalAccount` | id, display name, provenance, currency, optional starting balance, PnL convention. |
| `TradeAnnotation` | tags, star rating (1–5), review text, `planned_risk` (`Money`) — R-multiple is undefined without it (`has_risk()`). |
| `JournalNote` | Day-keyed, account-scoped free-form note. |
| `ClosedTrade` | One FIFO-matched round-trip: trade_id, instrument, side, quantity, entry/exit price, realized PnL, fees, holding period, long-term flag, exit date, optional `duration_ms`, annotation; helpers `net_pnl()`, `is_win/loss/breakeven()`, `gross_profit()`, `gross_loss()`, `realized_r()`. |

`src/store.rs` — `JournalStore`: in-memory `HashMap`-backed collection of
journal accounts, trade annotations, day-keyed notes, and closed trades with
sorted deterministic iteration and `StoreError` (`AccountNotFound`,
`AccountAlreadyExists`, `AnnotationNotFound`).

## Phase 2 — performance metrics engine + edge score

`src/metrics.rs`:

- `MetricsInput { closed_trades, open_trades, equity_curve, as_of }` —
  `as_of` is caller-injected; the `new()` convenience defaults it to
  `Utc::now()`, which is tracked as
  [gap 0005](../gaps/0005-performance-metrics-clock-query.md).
- `compute_metrics(input)` → `PerformanceMetrics`: total/closed/open trades,
  wins/losses/breakevens, net/gross PnL, fees, win rate, day win rate, trading
  days, profit factor (with an explicit `is_infinite` flag), avg win/loss,
  avg win/loss ratio, expectancy, largest win/loss, streaks (max win/loss,
  current), total volume, avg duration, max drawdown (amount + pct from the
  equity curve), recovery factor, profit concentration, avg realized R,
  trades-with-risk.
- `compute_edge_score(metrics)` → `EdgeScore { version, score, components,
  closed_trades }` — composite 0–100; `None` until `MIN_CLOSED_TRADES_FOR_SCORE`
  (5) closed trades. The six components (win rate, profit factor, avg
  win/loss, drawdown, recovery, consistency) are scored 0–100 and blended with
  equal weights — a documented **v1 placeholder formula** (LuxAlgo's proprietary
  weighting is not reverse-engineered), tracked as
  [gap 0006](../gaps/0006-performance-edge-score-v1-placeholder.md).

## Phase 3 — daily P&L calendar & account comparison

`src/calendar.rs`:

- `DailyPnl` — per-day bucket: date, net/gross PnL, fees, trade count,
  wins/losses/breakevens, volume.
- `build_calendar(trades, year, month)` → `PnlCalendar { year, month, weeks }`
  — weekly grid (Vela calendar shape) with empty buckets for partial weeks.
- `daily_pnl_buckets(trades)` and `total_fees(trades)` helpers.

`src/compare.rs`:

- `AccountMetricsView { account_id, range_from, range_to, metrics, edge_score }`.
- `compare_accounts(a_id, trades_a, b_id, trades_b, as_of, is_prior_period)` →
  two-sided `AccountComparison`.
- `compare_account_vs_prior(account_id, trades, boundary, as_of)` — splits an
  account's trades at `boundary` (prior = `exit_date < boundary`) and compares.

## Zero-I/O, zero-floats, zero-async

Pure synchronous domain library: no network, no filesystem, no clock queries;
all PnL, R-multiple, ratio, and edge-score figures are exact
`rust_decimal::Decimal` (`avg_duration_ms` is a timing figure carried as
`Option<u64>`).

## Related

- `vyapaar-ledger-portfolio` — `MatchedLot` / `AccountState::match_lots` source
  of the closed-trade stream.
- Open work is planned in
  [`docs/plans/0004-performance-journal-edge-and-determinism.md`](../plans/0004-performance-journal-edge-and-determinism.md).
- Tests: `model_tests.rs` (10), `store_tests.rs` (10), `metrics_tests.rs` (12),
  `calendar_tests.rs` (6), `compare_tests.rs` (4).
