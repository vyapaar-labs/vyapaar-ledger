# Gap 0007 — Portfolio `ClearingCalendar::new()` Hardcodes NSE Holidays 2024–2026

**Status**: open — tracked by [plan 0005](../plans/0005-settlement-and-fee-consolidation.md)

## What's Hardcoded

`crates/vyapaar-ledger-portfolio/src/lib.rs`:

- `ClearingCalendar::new()` seeds a hardcoded NSE trading-holiday table for
  **2024, 2025, and 2026** inside the domain crate
  (`add_nse_holidays_2024_2026`), and copies it verbatim to settlement
  holidays.
- The injection escape hatch exists — `ClearingCalendar::with_holidays(&[NaiveDate])`
  — and is used by a backtesting `NseTradingCalendar` to seed real NSE holidays
  and Muhurat sessions, but the domain crate still owns a diverging,
  static-by-year table.

## Why

- Holiday data is reference data, not domain logic: it belongs in a data
  provider (e.g. `vyapaar-data`, whose calendar gaps are filed there) rather
  than being frozen into a domain crate every year.
- A static table silently goes stale (new Muhurat sessions, one-off closures),
  and a second copy of the same table risks drift (`with_holidays` doc notes
  exactly this).

## How to Close It

1. Remove the hardcoded `add_nse_holidays_2024_2026` table (or shrink it to a
  documented test fixture).
2. Make `ClearingCalendar::new()` construct an empty calendar (weekend
  exclusion only), with real holidays always injected via `with_holidays`.
3. Ensure the backtest `NseTradingCalendar` (or `vyapaar-data`) seeds the real
  table, citing this gap.
4. Keep the existing `clearing_calendar_injection_tests.rs` behavior and extend
  it for empty defaults.

## Verification

`cargo test --workspace --profile prod` green; `grep -rn "2024\|2025\|2026"
crates/vyapaar-ledger-portfolio/src/lib.rs` returns no holiday table.
