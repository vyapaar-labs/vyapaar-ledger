# `vyapaar-ledger-oms` — Order Management System Reference

Documents `crates/vyapaar-ledger-oms/src/lib.rs`: the pure synchronous OMS state
machine owning the SEBI order lifecycle, idempotency, fill reconciliation, and
the OMS context canvas for pre-submission checks.

## `OrderStateMachine`

A deterministic, zero-async finite state machine governing order transitions.
Orders are keyed by the canonical [`ClientOrderId`], guaranteeing idempotency:
registering the same client order ID twice is rejected.

```
PendingNew → New → PartiallyFilled → Filled / Cancelled / Rejected / Expired
```

### Legal transition graph (`OrderStateMachine::transition`)

| From | To (legal) |
|------|-----------|
| `PendingNew` | `New`, `Rejected`, `Cancelled`, `PartiallyFilled` (a first fill may land before the ack) |
| `New` | `PartiallyFilled`, `Filled`, `Cancelled`, `Rejected` |
| `PartiallyFilled` | `Filled`, `Cancelled` |
| `Filled` / `Cancelled` / `Rejected` / `Expired` | itself only (terminal states are absorbing) |

Anything else returns `TransitionError::Illegal { from, to }`.

### Public API

| Method | Purpose |
|--------|---------|
| `register_order(entry)` / `register_request_ids(...)` | Register a fully-formed `OrderEntry` (or from request identifiers), rejecting duplicates. |
| `submit_order(request, now)` | Validate and register an `OrderRequest`; returns an `OrderAck` in `PendingNew`. Pure sync: order ID is derived deterministically as `OMS-<correlation-uuid>`; no UUID minting, no clock. |
| `cancel_order(client_order_id, now)` | Cancel an active order; terminal orders are rejected (`OrderNotActive`). |
| `process_fill(client_order_id, qty, now)` | Reconcile a fill against the target: transitions to `PartiallyFilled` while below target, `Filled` at target; returns `(state, fully_filled)`. Fills that would overshoot the target are rejected (`FillExceedsTarget`), never silently saturated. |
| `get_state` / `get_cumulative_fill` / `get_target_quantity` / `is_terminal` / `all_order_ids` | Read-side queries. |

### `OrderEntry`

Carries both identifier spellings of an order — the canonical string
`ClientOrderId` key and the correlation `Uuid` embedded in the wire request —
plus the confirmed `target_quantity`, cumulative `filled_quantity` (never
exceeding target), and caller-injected `created_at` / `updated_at`.

## OMS context types

`MarketData`, `OmsConfig`, `PortfolioRiskState`, and `OmsSnapshot` are the canvas
that the risk gate and broker adapters fill in around the state machine:

| Type | Fields / Purpose |
|------|------------------|
| `MarketData` | Instrument quote snapshot: `bid`, `ask`, `last`, `volume`, caller-stamped `timestamp` (for price-collar validation). |
| `OmsConfig` | Knobs: `max_notional_per_order`, `max_quantity_per_order`, `max_daily_loss`, `price_collar_bps`, `default_time_in_force`. |
| `PortfolioRiskState` | Pre-submission portfolio view: `cash`, positions map, `today_pnl`, `max_daily_loss`, `price_collar_bps`. |
| `OmsSnapshot` | Point-in-time counters: `total_orders`, `total_filled`, `total_cancelled`, `total_commission`, `total_fees`, `today_pnl`. |

These types are defined but not yet wired into OMS validation — tracked as
[`docs/gaps/0011-oms-context-types-unwired.md`](../gaps/0011-oms-context-types-unwired.md).

## Errors

| Error | Meaning |
|-------|---------|
| `OrderError::QuantityExceedsLimit` | Request quantity above `MAX_QUANTITY_PER_ORDER` (1 000 000). |
| `OrderError::DailyLossLimitExceeded` | `today_pnl < -max_loss`. |
| `OrderError::InvalidParameters` | Zero quantity, non-positive fill, negative commission. |
| `OrderError::OrderNotFound` / `OrderNotActive` | Unknown or terminal order addressed. |
| `OrderError::FillExceedsTarget` | Fill would push cumulative above the confirmed target. |
| `OrderError::DuplicateClientOrderId` | Idempotency violation. |
| `OrderError::BrokerError` | Broker/venue plumbing failure surfaced through the OMS. |
| `TransitionError::Illegal` / `DuplicateClientOrderId` | Transition-graph and registration violations. |

## Pure validation & acknowledgement builders

| Function | Purpose |
|----------|---------|
| `validate_order_request(request)` | Rejects zero quantity and quantities above `MAX_QUANTITY_PER_ORDER`. |
| `validate_order_params(qty, today_pnl, max_loss)` | Quantity ceiling + daily-loss ceiling. |
| `validate_fill_report(fill_quantity, commission, fees)` | Non-positive fill / negative commission rejection. |
| `submit_order(request, now)` | Stateless order-submission validation returning an `OrderAck`. |
| `build_order_ack(request, now)` | Deterministic ack builder (order ID from the correlation UUID). |
| `platform_order_id(client_order_uuid)` | `OMS-<correlation-uuid>` deterministic order ID. |

## Zero-I/O & temporal determinism

- No `SystemTime::now()` / `Instant::now()` — every timestamp is caller-injected.
- No filesystem, network, or database I/O; no `async`/`await`, no `tokio`.
- Replaying identical timestamped events produces byte-for-byte identical
  state transitions.

## Related

- `vyapaar-core/src/order.rs` — `OrderRequest`, `OrderSide`, `OrderType`,
  `OrderStatus`, `OrderAck`, `FillReport`, `TimeInForce`.
- `vyapaar-core/src/types.rs` — `Price`, `Quantity`, `InstrumentId`, `Money`.
- `vyapaar-ledger-portfolio` — FIFO tax-lot matching consumes `FillReport`s from
  the OMS.
- Tests: `crates/vyapaar-ledger-oms/tests/unit_tests.rs` (25 tests).
