# Gap 0008 — Corporate-Action Adjustments Are Aggregate-Level, Not Per-Lot

**Status**: open — tracked by [plan 0005](../plans/0005-settlement-and-fee-consolidation.md)

## What's Simplified

`crates/vyapaar-ledger-portfolio/src/lib.rs` applies corporate actions at the
aggregate-position level, not per FIFO `TaxLot`:

- `adjust_for_split` adjusts `pos.quantity` / `pos.entry_price` and notes
  "In a full implementation with individual lot tracking, each lot would be
  adjusted" — `TaxLot` quantities/prices are left untouched.
- `adjust_for_demerger` allocates cost basis with a single percentage and notes
  "This is approximate; full implementation would track per-lot"; the original
  position's entry price is rewritten to `(1 − pct) × price` rather than each
  lot's basis being split.
- `apply_corporate_action` / `apply_corporate_actions_to_position` / `..._to_account`
  operate over `HeldPosition`s only.

## Why

- FIFO cost-basis integrity lives in the `TaxLot` book: if split/bonus/demerger
  adjustments are not mirrored onto every lot's quantity, entry price, and
  remaining quantity, downstream `match_lots` realizes the wrong per-lot PnL
  and capital-gain classification.
- The aggregate-level approximation is fine for valuation snapshots but breaks
  exact tax-lot accounting.

## How to Close It

1. Add per-lot corporate-action adjustments (scale each lot's `quantity` and
  `entry_price` for split/bonus; allocate each lot's basis for demerger).
2. Keep the aggregate-level helpers as a thin projection over the adjusted lot
  book (or replace them).
3. Extend `apply_fill_tests` / `unit_tests` with per-lot cost-basis assertions
  after each action type.

## Verification

`cargo test --workspace --profile prod` green; per-lot cost basis after a
split/demerger matches the exact-arithmetic expectation in golden vectors.
