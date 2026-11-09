# `vyapaar-ledger` — Migration Verification Milestone

Verification commands for the `honba-kokoro → vyapaar-ledger` migration:

```bash
cd /home/pmallapp/Devel/Finance/vyapaar-labs/vyapaar-ledger
grep -rn "#\[cfg(test)\]" crates/*/src/ && exit 1 || true
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace
```

## Migration Milestone Status

| Milestone | Status |
|-----------|--------|
| Workspace manifest purity (no tokio, canonical deps) | Complete (`e22c95f`) |
| OMS pure synchronous state machine | Complete (`08112a8`) |
| Portfolio FIFO tax-lot on `AccountState` aggregate | Complete (`7c56840`) |
| Journal double-entry ledger + `AccountCode` | Complete (`2019ae6`) |
| Facade re-exports + repository traits | Complete (`5b6a051`) |
| Zero inline `#[cfg(test)]` in `src/` | Complete |
| `cargo fmt --check` | Clean |
| `cargo clippy --all-targets -- -D warnings` | Clean |
| `cargo test --workspace` (123 tests) | Passing |
