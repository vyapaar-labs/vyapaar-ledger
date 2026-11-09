# 0002 — Trade performance journal: annotations & analytics engine

**Status**: Proposed

> **Naming-collision warning (read this first)**: this repo already has a crate named
> `crates/vyapaar-ledger-journal` — its `Cargo.toml` description is "Double-entry general ledger
> and Indian statutory tax calculator (STT, GST, stamp duty, SEBI fees)", and its `src/lib.rs` doc
> comment is titled `व्यापार・खातावही vyapaar-ledger-journal` ("खातावही" = ledger-book). In that
> crate, "journal" means the double-entry **accounting** ledger. This plan is about a completely
> different concept — a trader's **performance/annotations journal** (notes, tags, star ratings,
> win-rate/profit-factor analytics, a P&L calendar) — and must **not** name its crate anything
> containing "journal", to avoid colliding the two meanings further. This plan proposes the name
> **`vyapaar-ledger-performance`**, which matches this workspace's existing
> `vyapaar-ledger-<subdomain>` convention (`vyapaar-ledger-oms`, `vyapaar-ledger-portfolio`,
> `vyapaar-ledger-journal`, `vyapaar-ledger-allocation`).

## 0. Executive Summary & Objective

A sibling repo, `vyapaar-ui-web` (Next.js web UI, not a submodule of this workspace), is building
a trade-performance analytics screen modeled on vela.luxalgo.com's "Journal" feature: win rate,
profit factor, expectancy, a composite 0-100 "edge score," drawdown/recovery, R-multiples,
win/loss streaks, and a daily-P&L calendar heatmap — plus free-text user annotations on trades
(notes, tags, a star rating, a review field). None of this exists anywhere in this workspace
today.

The objective of this plan is to design a new, pure, synchronous domain crate —
**`vyapaar-ledger-performance`** — that owns:
1. The data model for journal accounts, trade annotations, and day-keyed notes.
2. The pure analytics functions that compute Vela-equivalent performance metrics, an edge-score
   composite, daily P&L calendar aggregation, and account-vs-account/account-vs-prior-period
   comparison.

This crate is explicitly a **thin layer on top of `vyapaar-ledger-portfolio`**, not a
re-implementation of FIFO lot matching. `vyapaar-ledger-portfolio` already performs FIFO
round-trip derivation and realized-P&L computation via `AccountState::match_lots` (returning
`Vec<MatchedLot>`) and exposes `Trade`, `HeldPosition`, `TaxSummary`, `PortfolioSnapshot`, and
`PortfolioState`. The new crate consumes that derived trade/lot stream; it does not own fills or
lot-matching itself.

## 1. Context & Gaps Covered

- **Consumer (UI)**: `vyapaar-ui-web/docs/plans/0011-vela-journal-calendar-reports-domain-model.md`
  — the Next.js UI plan that motivates this work. It specifies the Vela-equivalent screens
  (Overview, Calendar, Compare, Notes, Breakdown) this crate's analytics must be able to back.
- **Downstream consumer (REST API)**: `vyapaar-runtime/docs/plans/0003-trade-journal-and-market-reference-rest-endpoints.md`
  (being written in parallel) — proposes new REST endpoints in `vyapaar-runtime-api-rest` that
  will call into whatever this plan's crate exposes. This plan only owns the pure domain crate;
  wiring it to HTTP is that plan's concern, not this one's (see §3 "How vyapaar-runtime will
  consume this").
- **Gaps covered**: no gap docs exist yet for this functionality in `vyapaar-ledger` (this is a
  net-new plan, not a cleanup of a previously-filed gap). Should implementation of any phase below
  need to stub or hardcode something (e.g. a placeholder edge-score blend formula), file the
  corresponding `docs/gaps/XXXX-*.md` in this repo at that time, per `.agents/rules/gap-tracking.md`.

### What already exists (verified, current type signatures from `crates/vyapaar-ledger-portfolio/src/lib.rs`)

```rust
pub struct TaxLot { /* lot-level cost basis */ }
pub struct MatchedLot { /* result of FIFO-matching a closing fill against open lots —
                            this IS "the round trip, derived for you" that Vela's UI promises */ }
pub struct LotFill { /* matching-input type */ }

pub struct HeldPosition {
    pub realized_pnl: /* Decimal-based */,
    pub updated_at: DateTime<Utc>,
    // ...
}

pub struct Trade {
    pub trade_date: NaiveDate,
    pub settle_date: NaiveDate,
    pub instrument: InstrumentId,
    pub side: PositionSide,
    pub quantity: u64,
    pub price: Price,
    pub cash_flow: i64, // minor units (paise)
}

pub struct AccountState {
    pub fn record_lot(&mut self, lot: &TaxLot);
    pub fn match_lots(&mut self, fill: &LotFill) -> Vec<MatchedLot>; // FIFO engine entry point
    // ...
}

pub enum CapitalGainType { /* short-term / long-term (Indian STCG/LTCG) */ }
pub struct TaxSummary { pub fn from_fills(/* ... */) -> Self; }
pub struct PortfolioSnapshot { /* ... */ }
pub struct PortfolioState { pub fn new(base_currency: Currency) -> Self; /* ... */ }
```

Re-exported at the `vyapaar-ledger-portfolio` crate root: `Currency, InstrumentId, Money,
OrderSide, PositionSide, Price, Quantity` (all from `vyapaar_core`, the shared kernel).

### Required repo conventions this crate must honor

- **Zero-I/O, zero-tokio, zero-async**: a pure synchronous domain library, matching this
  workspace's `[workspace]` `Cargo.toml` comment ("every member of this workspace is a pure,
  synchronous domain library ... temporal determinism is enforced by passing `now: DateTime<Utc>`
  explicitly").
- **Zero floats**: `rust_decimal::Decimal` only, for P&L, R-multiple, and every monetary or ratio
  figure that isn't inherently a plain count.
- **TDD + parallel test directory**: all tests in `crates/vyapaar-ledger-performance/tests/`,
  never `#[cfg(test)] mod tests` inside `src/`.
- **Dependencies**: `vyapaar-core` (workspace dependency, already listed in the root
  `Cargo.toml`'s `[workspace.dependencies]`) and `vyapaar-ledger-portfolio` (same-workspace path
  dependency) — consuming its `Trade`/`MatchedLot`/`PortfolioSnapshot`/`AccountState` types rather
  than redefining them. This is the layering point: performance analytics reads portfolio's
  derived trade/lot data; it does not own fills or lot-matching.

## 2. Component Specifications & Phases

### Phase 1 — Crate scaffold, journal-account entity, trade-annotation layer, day-keyed notes

Data model only, no analytics yet.

- **Journal account entity**: a grouping/provenance wrapper distinct from this repo's own
  OMS/portfolio `AccountState`. Mirrors a real distinction in Vela's own live API: a *trading*
  account, a *brokerage connection*, and a separate *journal* account used only for analytics are
  three different things — even a journal account "synced from a broker" is its own ledger, not a
  live-execution account. Proposed shape:
  ```rust
  pub struct JournalAccountId(/* newtype, e.g. uuid or u64 */);

  pub enum AccountProvenance {
      Manual,
      Imported,
      BrokerSynced,
  }

  pub struct JournalAccount {
      pub id: JournalAccountId,
      pub display_name: String,
      pub provenance: AccountProvenance,
      pub currency: Currency,
      pub starting_balance: Option<Money>,
      pub pnl_convention: PnlConvention,
  }
  ```
  **Open decision, resolved here**: Vela's own UI offers "FIFO (first in, first out)" as a named,
  presumably swappable profit-calculation convention. This repo's `AccountState::match_lots` is
  already FIFO-only. **v1 hard-codes FIFO** (recommended: simplest, matches the existing engine)
  and defers LIFO/other conventions to a later phase. `PnlConvention` is still modeled as an enum
  with a single `Fifo` variant today, so the type doesn't need to change shape when a second
  convention is added later — only a new variant plus a second code path.

- **Trade annotation layer**: attached per closed round-trip (per `MatchedLot`/derived `Trade`).
  ```rust
  pub struct TradeAnnotation {
      pub tags: Vec<String>,
      pub rating: Option<u8>,        // star rating
      pub review: Option<String>,    // long-form text
      pub planned_risk: Option<Money>,
  }
  ```
  `planned_risk` is a genuinely new input this repo's domain doesn't currently capture anywhere:
  R-multiple (realized P&L ÷ planned risk) is mathematically undefined without a user-entered risk
  amount per trade. Vela's own Trades table has `RATING`, `REVIEW`, and `R` columns, and its
  overview metrics include `avgRealizedR` and `tradesWithRisk` — all of which depend on this field
  being present and user-supplied, not derived.

- **Day-keyed journal notes**: separate from trade annotations — independent of any specific
  trade (Vela's own Notes screen: "Add one here or from the Journal drawer, place a Journal Note
  on the chart, or write notes on a trade" — notes can exist with no trade reference at all).
  ```rust
  pub struct JournalNote {
      pub account_id: JournalAccountId,
      pub day: NaiveDate,
      pub text: String,
  }
  ```

### Phase 2 — Performance metrics engine + edge-score composite

Pure functions computing, over a date range and an optional previous-period comparison, the
following metric set. Verified against vela.luxalgo.com's own live `/api/journal/overview` API
response shape — quoted here as the target contract:

```json
{
  "totalTrades": 0, "closedTrades": 0, "openTrades": 0, "wins": 0, "losses": 0, "breakevens": 0,
  "netPnl": 0, "grossPnl": 0, "fees": 0,
  "winRate": null, "dayWinRate": null, "tradingDays": 0,
  "profitFactor": null, "profitFactorIsInfinite": false,
  "avgWin": null, "avgLoss": null, "avgWinLossRatio": null, "expectancy": null,
  "largestWin": 0, "largestLoss": 0, "maxWinStreak": 0, "maxLossStreak": 0, "currentStreak": 0,
  "totalVolume": 0, "avgDurationMs": null,
  "maxDrawdown": 0, "maxDrawdownPct": null, "recoveryFactor": null,
  "profitConcentration": null, "avgRealizedR": null, "tradesWithRisk": 0
}
```

Plus a composite "edge score" (0-100, `null` until enough closed trades exist — Vela's own
empty-state message: "5 more closed trades needed") built from exactly 6 named components, each
independently scored 0-100:

```json
{"winRate": 0, "profitFactor": 0, "avgWinLoss": 0, "drawdown": 50, "recovery": 0, "consistency": 0}
```

- The exact per-component scoring formula and the final blend-into-one-number formula are **not**
  invented here — this is a legitimate open design question flagged for implementation time, not
  resolved by this plan. LuxAlgo's proprietary weighting is not reverse-engineered here.
- `maxDrawdown`/the `drawdown` component need a running-equity-peak calculation over the account's
  equity-curve history. This repo's `PortfolioState`/`PortfolioSnapshot` types are well-positioned
  to already support this (a time series of snapshots is exactly a running-equity-peak input);
  the metrics engine should consume a `PortfolioSnapshot` stream for this rather than
  re-deriving equity from trades alone.

### Phase 3 — Daily P&L calendar aggregation + account comparison

- **Daily P&L calendar aggregation**: one row per calendar day. Verified shape, from Vela's own
  `/api/journal/calendar?month=YYYY-MM` response:
  ```json
  {"date": "YYYY-MM-DD", "netPnl": 0, "grossPnl": 0, "fees": 0, "trades": 0, "wins": 0, "losses": 0, "breakevens": 0, "volume": 0}
  ```
  This is a straightforward group-by-day reduction over closed trades/fills — each day bucket is
  computable from the same `Trade`/`MatchedLot` stream the Phase 2 metrics engine already
  consumes. No new data model is needed beyond this aggregation function.

- **Account comparison**: given two journal accounts (or one account vs. its own prior period),
  return the Phase 2 metrics-engine output for each side so a UI can diff them (Vela's own Compare
  screen: "Two performances, same metrics," with a `vs account` / `vs previous` toggle). This is
  mostly a call-the-metrics-engine-twice concern, not new computation — kept short deliberately:
  the comparison function's job is just to run the Phase 2 engine over two `(account, date_range)`
  pairs and return both results side by side.

### Phase 4 — explicitly out of scope for v1

A "breakdown" view grouping P&L by symbol/tag/setup/session (Vela's own Breakdown screen). This is
**not designed here** because it was only observed in its empty state ("No closed trades in this
range to break down") during research — its exact grouping dimensions were never confirmed against
a live, populated response, so designing its shape now would mean guessing blind. Revisit once a
populated reference response is available.

## 3. How `vyapaar-runtime` will consume this

A REST crate in a sibling repo, `vyapaar-runtime-api-rest`, will wrap this crate's pure functions
behind new HTTP endpoints. That wiring is a separate concern, tracked by its own plan —
`vyapaar-runtime/docs/plans/0003-trade-journal-and-market-reference-rest-endpoints.md` — being
written in parallel. This plan owns only the pure domain crate (`vyapaar-ledger-performance`); it
does not design any HTTP surface.
