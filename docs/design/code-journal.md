# `vyapaar-ledger-journal` — Double-Entry Journal Reference

Documents `crates/vyapaar-ledger-journal/src/lib.rs`: the immutable double-entry
general ledger, the Indian chart of accounts, and the statutory tax calculator.

## `AccountCode`

The strongly typed chart-of-accounts code owned by this crate (single spelling
of every account):

| Constant | Code |
|----------|------|
| `CASH_AT_BROKER` | `Assets:Cash:BrokerAccount` |
| `HOLDINGS_EQUITY_DELIVERY` | `Assets:Holdings:EquityDelivery` |
| `LIABILITIES_MARGIN_PAYABLE` | `Liabilities:MarginPayable` |
| `OPENING_CAPITAL` | `Equity:OpeningCapital` |
| `EXPENSES_BROKERAGE` | `Expenses:Brokerage` |
| `EXPENSES_TAXES_STT` | `Expenses:Taxes:STT` |
| `EXPENSES_TAXES_GST` | `Expenses:Taxes:GST` |
| `EXPENSES_TAXES_STAMP_DUTY` | `Expenses:Taxes:StampDuty` |
| `EXPENSES_EXCHANGE_FEES` | `Expenses:ExchangeFees` |
| `EXPENSES_SEBI_FEES` | `Expenses:SEBIFees` |
| `EXPENSES_DP_CHARGES` | `Expenses:DPCharges` |
| `INCOME_REALIZED_PNL` | `Income:RealizedPnL` |

## `DoubleEntryLedger`

An append-only ledger of `Transaction` entries keyed by `AccountCode`. Every
transaction must balance (`sum(debits) == sum(credits)`), enforced at `post()`
via `post_transaction` validation.

### Public API

| Method | Purpose |
|--------|---------|
| `post(tx)` | Validate and post a balanced transaction; rejects unbalanced or non-positive legs (`LedgerError::InvalidAmount`). |
| `total_debits()` / `total_credits()` | Running totals. |
| `net_debit_balance(account)` / `net_credit_balance(account)` | Per-account net position. |
| `cash_balance()` / `holdings_balance()` / `opening_capital()` / `realized_pnl()` / `brokerage()` / `statutory_charges()` | Balance-sheet convenience reads over `AccountCode`. |
| `verify_accounting_identity()` | `Cash + Holdings == OpeningCapital + RealizedPnL − StatutoryCharges − Brokerage`. |
| `verify_double_entry_balance()` | `sum(Debits) == sum(Credits)`. |
| `record_opening_capital(tx_id, now, amount)` | Helper posting capital deposit. |
| `record_buy(tx_id, now, notional, &TaxBreakdown)` | Helper posting an equity buy with itemized statutory charges. |
| `record_sell(tx_id, now, proceeds, cost_basis, &TaxBreakdown)` | Helper posting an equity sell with cost-basis relief and realized PnL. |

### `Transaction` shape

`{ id, timestamp, debits: BTreeMap<AccountCode, Decimal>, credits: BTreeMap<AccountCode, Decimal>, description }`.

## `TaxCalculator` — statutory fees

Two tracks coexist (see [`docs/gaps/0010-journal-golden-framework-legacy-fees.md`](../gaps/0010-journal-golden-framework-legacy-fees.md)):

- **Legacy floor-rounded methods** (retained verbatim for the paise-denominated
  contract-note fixtures): `stt`, `GST`, `stamp_duty`, `sebi_turnover_charges`,
  `dp_charges`.
- **Canonical methods** (round half-even to 2 dp via `round_inr`): `brokerage`
  (0.03% of notional capped at ₹20), `nse_turnover_charges` (0.00297%),
  `bse_turnover_charges` (0.00375%), `sebi_fee` (₹10/crore), `stt_charge`
  (side-and-product gated: Delivery 0.1% both sides; Intraday 0.025%, Futures
  0.0125%, Options 0.0625% on the sell leg only), `stamp_duty_charge` (buy side
  only, per product), `dp_charge` (₹13.50 flat on the first delivery sell of the
  day), `gst_charge` (18% on brokerage + exchange + SEBI + DP).

The single entry point `compute_charges(side, product, exchange, price,
quantity, is_first_sell_of_day)` returns an itemized `TaxBreakdown`. Exchange
selection maps `Exchange::Bse` to the BSE turnover rate and every other segment
(NSE, NFO, MCX, CDS) to the NSE cash-segment rate — per-segment fee tables are
a follow-on (see [`docs/gaps/0009-journal-per-segment-fee-tables.md`](../gaps/0009-journal-per-segment-fee-tables.md)).

### `TaxBreakdown`

`{ brokerage, stt, stamp_duty, exchange_turnover, sebi_turnover, gst, dp_charge }`
with `total()` and `add_assign(&other)`.

## Related

- [`code-contract-notes.md`](code-contract-notes.md) — the broker contract-note
  golden framework in `contract_notes.rs`.
- `vyapaar-ledger/src/repository.rs` — `LedgerRepository` trait (`post_entry`,
  `get_balance`) and `InMemoryLedgerRepository`; `JournalTransaction` /
  `PostingEntry` / `PostingSide` facade projections.
- `vyapaar-core/src/order.rs` — `OrderSide` (canonical serde UPPERCASE).
- Tests: `unit_tests.rs` (18), `tax_calculator_tests.rs` (27),
  `ledger_proptest_invariants.rs` (8, property-based),
  `golden.rs` (5), plus `benches/golden_tests.rs`.
