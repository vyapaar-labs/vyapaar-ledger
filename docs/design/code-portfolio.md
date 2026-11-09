# `vyapaar-ledger` — FIFO Tax-Lot Portfolio Reference

Documents `crates/vyapaar-ledger-portfolio/src/lib.rs`: the FIFO tax-lot engine for
deterministic cost-basis calculation and realized/unrealized PnL.

## `AccountState`

The portfolio aggregate root per trading account. Owns the FIFO tax-lot matching logic
(`match_lots`) — the repository only adapts the canonical `FillReport` into the portfolio
matching input.

### FIFO Tax-Lot Matching

When a fill arrives, `AccountState::match_lots`:
1. Pops open (non-exhausted) lots in FIFO order (oldest first).
2. Matches the fill quantity against each lot until exhausted.
3. Returns `MatchedLot` entries with per-lot realized PnL.
4. Records a new lot for any remaining buy quantity (or adjusts for sell).

### Cost Basis & PnL

- **Realized PnL**: computed per matched lot as `(fill_price - lot_entry_price) * matched_qty`.
- **Unrealized PnL**: computed against current mark price for open lots.
- **Average entry price**: volume-weighted across open lots for the instrument.

## Key Types

| Type | Purpose |
|------|---------|
| `TaxLot` | Individual cost-basis lot (instrument, qty, entry price, timestamp) |
| `MatchedLot` | Result of matching a fill against one tax lot (realized PnL, matched qty) |
| `LotFill` | Adapted fill input for portfolio matching (instrument, side, price, qty, timestamp) |
| `PositionRecord` | Read-side projection: net qty, avg price, realized/unrealized PnL, open lots |

## Zero-Floats Policy

All prices, quantities, and PnL values use `rust_decimal::Decimal` — exact 128-bit
arithmetic with no `f32`/`f64` intermediates. Settlement computed down to the single
paisa.

## Related

- `vyapaar-ledger/src/repository.rs` — `InMemoryPositionRepository.match_lots()` delegates to `AccountState::match_lots()`.
- `vyapaar-core/src/order.rs` — `FillReport` consumed as matching input.
- `vyapaar-core/src/types.rs` — `Price`, `Quantity`, `Money`, `Currency`.
