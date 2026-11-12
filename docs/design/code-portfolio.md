# `vyapaar-ledger-portfolio` — FIFO Tax-Lot Portfolio Reference

Documents `crates/vyapaar-ledger-portfolio/src/lib.rs`: the FIFO tax-lot engine,
Indian T+1 Demat settlement, corporate actions, and portfolio valuation.

## `AccountState`

The portfolio aggregate root per trading account. Owns the canonical FIFO
tax-lot matching logic (`match_lots`) and the lot book (`record_lot`) — the
repository layer only adapts canonical `FillReport`s into the matching input.

### `AccountState::match_lots` (FIFO matching)

When a closing fill arrives, `AccountState::match_lots(&LotFill)`:

1. Iterates open (non-exhausted) lots in FIFO order (oldest first).
2. Matches only *opposing* sides — a `Sell` fill closes `Long` lots, a `Buy`
   fill closes `Short` lots (Indian equity delivery semantics).
3. Returns `MatchedLot` entries with per-lot realized PnL, holding period, and
   long-term flag (`holding_period_days > 365`).
4. Rolls the aggregate's realized PnL and quantity onto the matching
   `HeldPosition`; matched quantity never exceeds the fill.

### Cost basis & PnL

- **Realized PnL**: computed per matched lot — `(exit − entry) × qty` for
  `Long`, `(entry − exit) × qty` for `Short` (all `rust_decimal::Decimal`).
- **Unrealized PnL**: computed against current marks for open positions.
- **Average entry price**: carried per `HeldPosition.entry_price`.

## Settlement & corporate actions

| Type / Function | Purpose |
|-----------------|---------|
| `ClearingCalendar` | NSE/BSE trading and settlement holidays; `is_trading_day`, `is_settlement_day`, `settlement_date` (T+1 business day), `next/prev_trading_day`. `new()` seeds a hardcoded 2024–2026 NSE table (see [`docs/gaps/0007-portfolio-hardcoded-nse-calendar.md`](../gaps/0007-portfolio-hardcoded-nse-calendar.md)); `with_holidays` injects an external holiday list. |
| `Product` | Execution product taxonomy: `Delivery`, `Intraday`, `CoverOrder`, `BracketOrder`; `requires_settlement()` is true only for `Delivery`. |
| `DematHoldings` | Free vs blocked holdings, unsettled receivables/payables, `block_for_settlement`, `settle_receivable`. |
| `SettlementEngine` | Builds `SettlementInstruction`s from fills (`create_instruction`) and processes due settlements (`process_settlements`). |
| `Fill` / `SettlementInstruction` | Fill event and settlement instruction records (broker/execution layer shapes). |
| `CorporateAction` | `Split`, `Bonus`, `Dividend`, `Demerger` with ex-date; `apply_corporate_action` dispatches to `adjust_for_split`, `adjust_for_bonus`, `record_dividend`, `adjust_for_demerger`. Aggregate-level approximations — per-lot cost-basis adjustment is a follow-on (see [`docs/gaps/0008-portfolio-corporate-action-per-lot-cost-basis.md`](../gaps/0008-portfolio-corporate-action-per-lot-cost-basis.md)). |
| `settle_trades` | Trade-based T+1 settlement across a `PortfolioState` (cash movements, position changes). |
| `compute_capital_gains` / `CapitalGainType` / `TaxSummary` | FIFO capital gains with holding-period awareness; `TaxSummary::from_fills` builds a period summary applying corporate actions before FIFO cost basis. |

## Valuation & position math

| Function | Purpose |
|----------|---------|
| `Marks` | Market marks (instrument → last price) injected by the caller. |
| `value(state, marks)` | Deterministic `PortfolioSnapshot`: totals, per-symbol breakdown, unrealized PnL (against marks), realized PnL (every held position). |
| `apply_fill` / `apply_fill_to_account` | Legacy position-level fill application, including full-close-and-reverse (`Long`→`Short` and vice versa). |
| `init_position` | `HeldPosition` factory from a fill event. |
| `HeldPosition` | Position aggregate carrying `entry_price`, `entry_lot_ids`, `entry_date`, `realized_pnl`, caller-injected `updated_at`. |

## Key types

| Type | Purpose |
|------|---------|
| `TaxLot` | Canonical cost-basis lot: `lot_id`, `account_id`, `instrument`, `side`, `entry_price`, `quantity`, `remaining_quantity`, `entry_date`, `created_at`; `is_exhausted()`. |
| `MatchedLot` | Result of matching a fill against one tax lot (matched qty, entry/exit prices, realized PnL, holding period, long-term flag). |
| `LotFill` | Adapted fill input for portfolio matching (instrument, side, price, qty, timestamp). |
| `PortfolioState` | Multi-account state: `accounts: BTreeMap<String, AccountState>`, `base_currency`. |

## Zero-floats & determinism

All prices, quantities, and PnL values use `rust_decimal::Decimal` — no
`f32`/`f64` intermediates; settlement is computed down to the single paisa.
No clock and no I/O — the caller stamps `as_of`/`now`/`entry_date` so the same
positions and marks always give a byte-identical snapshot.

## Related

- `vyapaar-ledger/src/repository.rs` — `InMemoryPositionRepository` delegates
  `match_lots` / `record_lot` to the `AccountState` aggregate.
- `vyapaar-core/src/order.rs` — `FillReport` consumed as matching input.
- `vyapaar-core/src/types.rs` — `Price`, `Quantity`, `Money`, `Currency`.
- Tests: `apply_fill_tests.rs` (7), `clearing_calendar_injection_tests.rs` (6),
  `unit_tests.rs` (23), plus `benches/portfolio_benchmarks.rs`.
