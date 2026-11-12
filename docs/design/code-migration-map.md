# `vyapaar-ledger` — Historical Migration Map (honba-kokoro → vyapaar-ledger)

> **Tier**: Tier 1B (Depends only on `vyapaar-core`)
> **Source**: `honba-workspace/honba-kokoro`
> **Target**: `vyapaar-ledger`
> **Status**: Migration complete (all 5 Fixes merged).

## Sub-Crate Mapping Table

| Source Crate (`honba-kokoro/`) | Target Crate (`vyapaar-ledger/`) | Package Name in `Cargo.toml` |
| :--- | :--- | :--- |
| `crates/honba-kokoro-oms` | `crates/vyapaar-ledger-oms` | `vyapaar-ledger-oms` |
| `crates/honba-kokoro-portfolio` | `crates/vyapaar-ledger-portfolio` | `vyapaar-ledger-portfolio` |
| `crates/honba-kokoro-ledger` | `crates/vyapaar-ledger-journal` | `vyapaar-ledger-journal` |
| `crates/honba-kokoro` | `crates/vyapaar-ledger` | `vyapaar-ledger` |

## Mandatory DDD Fixes During Migration

### Fix 1: Workspace `Cargo.toml` Purity

- Remove `tokio` from `[workspace.dependencies]` in `vyapaar-ledger/Cargo.toml`.
- Add workspace members to `[workspace.dependencies]`:
  ```toml
  vyapaar-core = { path = "../vyapaar-core" }
  vyapaar-ledger = { path = "crates/vyapaar-ledger" }
  vyapaar-ledger-oms = { path = "crates/vyapaar-ledger-oms" }
  vyapaar-ledger-portfolio = { path = "crates/vyapaar-ledger-portfolio" }
  vyapaar-ledger-journal = { path = "crates/vyapaar-ledger-journal" }
  chrono = { version = "0.4", features = ["serde"] }
  uuid = { version = "1", features = ["v4", "serde"] }
  rust_decimal = { version = "1", features = ["serde-with-str"] }
  serde = { version = "1", features = ["derive"] }
  serde_json = { version = "1", features = ["float_roundtrip"] }
  thiserror = "1"
  proptest = "1"
  ```

### Fix 2: `vyapaar-ledger-oms` (`crates/vyapaar-ledger-oms/src/lib.rs`)

- Import `Exchange`, `InstrumentId`, `Money`, `Currency`, `Price`, `Quantity`, `OrderSide`, `OrderType`, `TimeInForce`, `OrderStatus` from `vyapaar_core`.
- Remove `#[tokio::test]` and `async fn submit_order` / `async fn cancel_order` (make pure synchronous state transitions accepting `now: DateTime<Utc>`).
- Move the entire `#[cfg(test)] mod tests` block into `crates/vyapaar-ledger-oms/tests/unit_tests.rs`.

### Fix 3: `vyapaar-ledger-portfolio` (`crates/vyapaar-ledger-portfolio/`)

- Copy `src/lib.rs`, `src/engine.rs`, `src/error.rs`, `src/marks.rs`, `src/state.rs` from `honba-kokoro-portfolio/src/`.
- Do **NOT** copy `src/tests/` into `src/`! Move tests into `crates/vyapaar-ledger-portfolio/tests/`.
- Replace `honba_core` with `vyapaar_core`.

### Fix 4: `vyapaar-ledger-journal` (`crates/vyapaar-ledger-journal/`)

- Copy `honba-kokoro-ledger/src/lib.rs` into `crates/vyapaar-ledger-journal/src/lib.rs`.
- Extract inline `#[cfg(test)] mod tests` and `src/golden_tests/mod.rs` into `crates/vyapaar-ledger-journal/tests/`.
- Replace `honba_kokoro_ledger` with `vyapaar_ledger_journal` in all test files.

### Fix 5: `vyapaar-ledger` Facade (`crates/vyapaar-ledger/`)

- Port `src/lib.rs`, `src/repository.rs`, `src/sizing.rs` from `honba-kokoro/crates/honba-kokoro/src/`.
- Update re-exports in `src/lib.rs`:
  ```rust
  pub use vyapaar_ledger_journal as journal;
  pub use vyapaar_ledger_journal as ledger;
  pub use vyapaar_ledger_oms as oms;
  pub use vyapaar_ledger_portfolio as portfolio;
  ```
- Port `tests/repository_and_sizing_tests.rs` into `crates/vyapaar-ledger/tests/`.

## Verification Commands

```bash
# Run from the repository root
grep -rn "#\[cfg(test)\]" crates/*/src/ && exit 1 || true
cargo fmt --check
cargo clippy --workspace --all-targets --profile prod -- -D warnings
cargo test --workspace --profile prod
```

## Migration Execution Status — Tier 1B complete

Every Fix in §2 is merged and the verification gate passes:
`cargo fmt --check`, `cargo clippy --workspace --all-targets --profile prod -- -D warnings`,
`cargo test --workspace --profile prod` (182 tests as of 2026-11-12), and zero
`#[cfg(test)]` in `crates/*/src/`. See
[`implementation-record.md`](implementation-record.md) for the full landing and
verification history.

| Fix | Commit | Delivered |
| :-- | :----- | :-------- |
| Fix 1 — workspace manifest purity | `e22c95f` | zero-I/O dependency manifest, canonical path deps, no tokio |
| Fix 2 — OMS | `08112a8` | pure synchronous order state machine, `target_quantity` recorded at registration |
| Fix 3 — portfolio | `7c56840` | canonical FIFO `TaxLot`/`MatchedLot` matching moved onto the `AccountState` aggregate |
| Fix 4 — journal | `2019ae6` | double-entry ledger keyed by the journal-owned `AccountCode`, pure contract-note module |
| Fix 5 — facade | `5b6a051` | canonical core/portfolio/journal re-exports, aggregate-delegating in-memory repositories |

Migration deltas beyond the letter of §2:
- Fix 3: orphaned legacy modules `engine.rs`, `error.rs`, `marks.rs` and `state.rs` were **not** ported as files; their live functionality survives under canonical names (`Marks`, `apply_fill`, `apply_corporate_action`, `settle_trades`, `PortfolioState`/`AccountState`).
- Fix 5: strongly typed ids and `FillReport` are the canonical `vyapaar_core` types (not local duplicates); `AccountCode` is owned by the journal crate; every timestamp and UUID is caller-injected — temporally deterministic by construction.
