# Gap 0011 — OMS Context Types Defined but Not Wired into Validation

**Status**: open — tracked by [plan 0005](../plans/0005-settlement-and-fee-consolidation.md)

## What's Declared but Unused

`crates/vyapaar-ledger-oms/src/lib.rs` defines four context types as "canvas the
risk gate and broker adapters fill in":

- `MarketData { instrument, bid, ask, last, volume, timestamp }`
- `OmsConfig { max_notional_per_order, max_quantity_per_order, max_daily_loss,
  price_collar_bps, default_time_in_force }`
- `PortfolioRiskState { cash, positions, today_pnl, max_daily_loss, price_collar_bps }`
- `OmsSnapshot { total_orders, total_filled, total_cancelled, total_commission,
  total_fees, today_pnl }`

None of them is referenced anywhere else in the workspace (no consumer, no
test). The OMS only enforces the quantity ceiling (`validate_order_request` /
`validate_order_params` with `MAX_QUANTITY_PER_ORDER`) and the daily-loss
ceiling — the configurable `max_notional_per_order`, `price_collar_bps`,
`max_quantity_per_order` (config), and the `MarketData` collar inputs are
declared but **not enforced**.

## Why

- Pre-submission risk checks (notional cap, price collar) are documented OMS
  responsibilities (module doc: "it validates, prices, and forwards order
  requests to brokers"); shipping the canvas types without wiring invites
  dead code and a false sense of coverage.

## How to Close It

1. Add an `OmsConfig`-driven validation entry point that enforces
  `max_notional_per_order` (from `MarketData`-stamped price) and the
  configured `max_quantity_per_order`.
2. Add a `price_collar_bps` check against `MarketData { bid, ask, last }`.
3. Make `OmsSnapshot` counters actually maintained (or remove the type and
  re-add it when a consumer exists).
4. Add tests covering each newly-enforced rule.

## Verification

`cargo test --workspace --profile prod` green with new collar/notional tests;
`grep -rn "OmsConfig\|MarketData\|PortfolioRiskState" crates/` shows consumers
beyond `src/lib.rs`.
