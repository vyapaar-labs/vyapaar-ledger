# Gap 0009 — Journal Exchange Turnover Fee Tables Fall Back to NSE Cash-Segment Rate

**Status**: open — tracked by [plan 0005](../plans/0005-settlement-and-fee-consolidation.md)

## What's Stubbed

`crates/vyapaar-ledger-journal/src/lib.rs`, `TaxCalculator::compute_charges`:

```rust
let exchange_turnover = match exchange {
    vyapaar_core::Exchange::Bse => Self::bse_turnover_charges(notional),
    // NSE, NFO, MCX and CDS all fall back to the NSE cash-segment rate.
    _ => Self::nse_turnover_charges(notional),
};
```

Only two turnover rates exist (`nse_turnover_charges` 0.00297%,
`bse_turnover_charges` 0.00375%). Every derivative segment — NFO, MCX, CDS —
is billed at the NSE cash-segment rate until "per-segment fee tables arrive
with real exchange data". The migration note in the crate module doc records
the same fallback.

## Why

- F&O, commodity, and CDS exchange transaction charges differ from the cash
  segment (and from each other); over-billing/under-billing at the cash rate
  corrupts statutory-charge totals and the contract-note parity tests.
- This is reference/tariff data, so it belongs in a table owned by a data or
  finance provider rather than hardcoded scalars.

## How to Close It

1. Add per-segment turnover fee tables (NFO, MCX, CDS) fed from an external
  data source (e.g. `vyapaar-market-finance::india::costs` or `vyapaar-data`).
2. Extend `compute_charges` to look up the segment rate instead of falling
  back to NSE cash.
3. Update the golden fixtures/tests for derivative segments and remove the
  fallback comment.
4. Add a test pinning each segment's rate.

## Verification

`cargo test --workspace --profile prod` green; `test_compute_charges_derivative_segments_fall_back_to_nse_rate`
is replaced by per-segment assertions.
