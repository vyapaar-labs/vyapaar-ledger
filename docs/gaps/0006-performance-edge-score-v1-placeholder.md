# Gap 0006 — Performance Edge Score Is a v1 Placeholder Formula

**Status**: open — tracked by [plan 0004](../plans/0004-performance-journal-edge-and-determinism.md)

## What's Stubbed / Simplified

`crates/vyapaar-ledger-performance/src/metrics.rs` implements the composite
`EdgeScore` (0–100) with:

- `EDGE_SCORE_VERSION = 1`
- `MIN_CLOSED_TRADES_FOR_SCORE = 5` (mirrors Vela's "5 more closed trades needed")
- six components — win rate, profit factor, avg win/loss, drawdown, recovery,
  consistency — each scored 0–100 with **equal weights**, blended by a simple
  average.

The module doc is explicit: "The plan flags the exact per-component scoring and
blend formula as an open design question (LuxAlgo's proprietary weighting is not
reverse-engineered). This v1 implementation uses a transparent, documented
formula with equal component weights; it is deliberately easy to replace."

## Why

- The edge score is the headline number of the performance journal; a
  placeholder formula is fine for a first slice but must not silently ship as
  final.
- Each component's scoring bracket (e.g. `score_win_rate` is a straight
  percentage, `score_profit_factor` lerps 0.5→2, `score_drawdown` subtracts
  50 per 2% drawdown) is an arbitrary v1 choice.

## How to Close It

1. Define the intended per-component scoring brackets and blend weights as a
  documented spec (with research support, or an explicit decision to keep equal
  weights).
2. Bump `EDGE_SCORE_VERSION` to 2 and implement the agreed formula.
3. Add golden-vector tests pinning the composite score for a known trade set.

## Verification

`cargo test --workspace --profile prod` green with the version-2 golden
vectors; the v1 formula is either superseded or documented as the final spec.
