# व्यापार・账簿 vyapaar-ledger

> *Domain Heart (Zero-I/O): Order Management System (OMS), FIFO tax-lot portfolio tracking, and double-entry general ledger.*

Zero-I/O pure state machine domain engine enforcing double-entry bookkeeping, multi-currency position tracking, and atomic OMS state transitions.

## Subcrates

- `vyapaar-ledger` — Facade and aggregate coordinator
- `vyapaar-ledger-oms` — Order Management System state machine & transitions
- `vyapaar-ledger-portfolio` — FIFO tax-lot accounting and position tracking
- `vyapaar-ledger-journal` — Double-entry general ledger and balance validation

## Build & Test

```bash
make prod
make check
make test
```

License: AGPL-3.0.
