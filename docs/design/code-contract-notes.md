# `vyapaar-ledger-journal` — Contract-Note Golden Framework Reference

Documents `crates/vyapaar-ledger-journal/src/contract_notes.rs`: the broker
contract-note golden-accounting framework, now a pure library module (the
filesystem loaders `load_from_csv` / `load_from_json` were dropped for the
zero-I/O gate; trades are added programmatically).

## `ContractNoteTrade`

One trade as recorded on a broker contract note:

`{ trade_id, broker, trade_time, symbol, exchange, side, quantity, price,
product_type, brokerage, stt_charged, exchange_charges, sebi_charges,
stamp_duty, gst, dp_charges, net_amount, settle_date }`.

### `ProductType`

The contract-note product taxonomy (`Delivery`, `Intraday`, `Options`,
`Futures`; serde UPPERCASE) — distinct from the portfolio crate's execution
`Product` and from any future SEBI retail segmentation.

## `GoldenTestRunner`

Pure data-in, report-out comparison engine:

- `new()` / `with_tolerance(paise)` / `add_trade(trade)` / `add_trade_mut(trade)`
  build the fixture set.
- `run()` compares each trade's statutory fields against the broker's recorded
  charges using the **legacy floor-rounded** `TaxCalculator` methods and
  produces a `GoldenTestReport { total, passed, failed, results,
  tolerance_paise }` with `pass_rate()`, `failed_trades()`,
  `stt_mismatches()`, `net_amount_mismatches()`, `to_json()`, and
  `print_summary()`.
- `ComparisonResult` carries per-field `*_match` / `*_calculated` /
  `*_broker` pairs plus `all_match` and `max_deviation_paise`.

> The runner compares with the legacy fee track; consolidating the golden
> framework onto the canonical `compute_charges` is tracked as
> [`docs/gaps/0010-journal-golden-framework-legacy-fees.md`](../gaps/0010-journal-golden-framework-legacy-fees.md).

## `CsvRecord`

Pure serde row shape for contract-note CSV data; `into_trade(broker)` maps the
UPPERCASE side (`BUY`/`SELL`) and product (`DELIVERY`/`CNC`, `INTRADAY`/`MIS`,
`OPTIONS`, `FUTURES`) strings onto the canonical enums. The `csv` file reading
itself stays outside this crate.

## Sample fixtures

All fixtures take an explicit `now: DateTime<Utc>` (no wall clock):

| Fixture | Trade |
|---------|-------|
| `sample_zerodha_delivery_buy_trade` | Buy 100 RELIANCE @ ₹2,500 (Delivery, NSE) |
| `sample_zerodha_intraday_sell_trade` | Sell 50 INFY @ ₹1,500 (Intraday, NSE) |
| `sample_upstox_delivery_buy_trade` | Buy 200 TCS @ ₹3,200 (Delivery, NSE) |
| `sample_upstox_option_buy_trade` | Buy 1 lot NIFTY 19000 CE @ ₹150 (Options, NFO) |
| `sample_total_charges_trade` | Buy 100 RELIANCE @ ₹2,000 (total-charges reconciliation) |
| `generate_sample_csv(broker, count, now)` | Deterministic CSV text writer (no `csv` crate dependency) |

## Related

- [`code-journal.md`](code-journal.md) — `TaxCalculator` (legacy + canonical),
  `TaxBreakdown`.
- Tests: `tests/golden.rs` (5), `tests/unit_tests.rs` golden-runner cases,
  `benches/golden_tests.rs`.
