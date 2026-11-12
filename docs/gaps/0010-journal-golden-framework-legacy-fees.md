# Gap 0010 — Golden Contract-Note Framework Runs on the Legacy Floor-Rounded Fee Track

**Status**: open — tracked by [plan 0005](../plans/0005-settlement-and-fee-consolidation.md)

## What's Duplicated / Inconsistent

`crates/vyapaar-ledger-journal` intentionally maintains **two** statutory-fee
tracks:

- **Legacy floor-rounded methods** (retained verbatim for the
  paise-denominated contract-note fixtures): `stt`, `GST`, `stamp_duty`,
  `sebi_turnover_charges`, `dp_charges`.
- **Canonical methods** (round half-even to 2 dp): `brokerage`,
  `nse/bse_turnover_charges`, `sebi_fee`, `stt_charge`, `stamp_duty_charge`,
  `dp_charge`, `gst_charge`, and the single entry point `compute_charges`
  returning `TaxBreakdown`.

The golden framework in `contract_notes.rs` (`GoldenTestRunner::compare_trade`)
recomputes charges with the **legacy** floor-rounded methods, so the golden
parity suite validates the legacy track — not the canonical `compute_charges`
track that `DoubleEntryLedger::record_buy`/`record_sell` and real callers use.

## Why

- Research-live parity and "single source of truth" demand one fee model.
- Fixes already landed in the canonical track (e.g. `stt_charge` correctly
  gates intraday STT to the sell leg; legacy `stt` charges it on buys too).
  The golden suite should be validating the canonical track, otherwise
  contract-note parity does not actually protect the shipped fee model.

## How to Close It

1. Switch `GoldenTestRunner::compare_trade` to `TaxCalculator::compute_charges`
  (`ProductType` → canonical parameters), keeping the 1-paise tolerance.
2. Re-baseline the sample fixtures (Zerodha/Upstox delivery/intraday/options)
  against the canonical track, documenting any intentional deviation.
3. Delete the legacy floor-rounded methods once no fixture depends on them
  (or gate them behind a `#[cfg(test)]`-style fixture module in `tests/`).
4. Add a parity test asserting the two tracks agree where they should.

## Verification

`cargo test --workspace --profile prod` green with the golden suite on the
canonical track; no production `src/` code calls the legacy methods.
