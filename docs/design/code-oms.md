# `vyapaar-ledger` — OMS State Machine Reference

Documents `crates/vyapaar-ledger-oms/src/lib.rs`: the pure synchronous Order Management
System state machine managing the SEBI order lifecycle.

## `OrderStateMachine`

A deterministic, zero-async finite state machine governing order transitions:

```
PendingNew → New → PartiallyFilled → Filled / Cancelled / Rejected / Expired
```

### Responsibilities

- **State transitions** driven by `register_order`, `process_fill`, `cancel`, `reject`, `expire`.
- **Idempotency** via `ClientOrderId` — duplicate registrations of the same client order ID are rejected.
- **Target vs cumulative quantity tracking**: `target_quantity` recorded at registration so `process_fill` accurately distinguishes `PartiallyFilled` vs `Filled`.
- **Pure sync**: no `async`/`await`, no `tokio`, no wall-clock queries — all timestamps injected explicitly via `DateTime<Utc>`.

## Key Types

| Type | Purpose |
|------|---------|
| `OrderState` | `PendingNew`, `New`, `PartiallyFilled`, `Filled`, `Cancelled`, `Rejected`, `Expired` |
| `OrderRecord` | Canonical order record (client_order_id, instrument, side, qty, price, status, timestamps) |
| `FillReport` | Execution confirmation (from `vyapaar_core`) — price, qty, exchange order ID, timestamp |
| `OrderRequest` | Canonical order submission (from `vyapaar_core`) |

## Zero-I/O & Temporal Determinism

- No `SystemTime::now()` or `Instant::now()` — every timestamp is caller-injected.
- No filesystem, network, or database I/O.
- Replaying identical timestamped events produces byte-for-byte identical state transitions.

## Related

- `vyapaar-core/src/order.rs` — `OrderRequest`, `OrderSide`, `OrderType`, `OrderStatus`, `OrderAck`, `FillReport`.
- `vyapaar-core/src/types.rs` — `Price`, `Quantity`, `InstrumentId`, `Money`.
- `vyapaar-ledger-portfolio` — FIFO tax-lot matching consumes `FillReport`s from the OMS.
