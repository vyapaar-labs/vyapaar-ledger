# `vyapaar-ledger` — Double-Entry Journal Reference

Documents `crates/vyapaar-ledger-journal/src/lib.rs`: the immutable double-entry general
ledger guaranteeing ledger integrity down to the single paisa.

## `DoubleEntryLedger`

An append-only ledger of `JournalTransaction` entries. Every transaction must balance:
`sum(debits) == sum(credits)` — enforced at construction via `JournalTransaction::validate()`.

### Double-Entry Invariant

```rust
pub fn validate(&self) -> Result<(), DomainError> {
    // debits non-empty, credits non-empty
    // each leg amount strictly positive
    // total_debit == total_credit (exact Decimal comparison)
}
```

### Key Types

| Type | Purpose |
|------|---------|
| `JournalTransaction` | Balanced transaction: id, timestamp, description, debits, credits |
| `PostingEntry` | Single debit/credit leg: `account` (`AccountCode`), `amount` (`Money`) |
| `PostingSide` | `Debit` / `Credit` |
| `AccountCode` | Chart of accounts code (owned by journal crate) |
| `LedgerError` | `UnbalancedLedger`, `Validation`, `NotFound`, `Storage` |

## Related

- `vyapaar-ledger/src/repository.rs` — `LedgerRepository` trait (`post_entry`, `get_balance`) and `InMemoryLedgerRepository`.
- `vyapaar-core/src/types.rs` — `Money`, `Currency`.
- `vyapaar-core/src/ids.rs` — `TransactionId`.
