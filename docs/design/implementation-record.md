# `vyapaar-ledger` — Implementation Record

Dismantled from the former `docs/roadmap/` (`roadmap.md` +
`migration-verification.md`). Completed milestones are recorded here; remaining
future work moved to [`docs/gaps/`](../gaps/README.md) and
[`docs/plans/`](../plans/README.md).

## Migration: `honba-kokoro` → `vyapaar-ledger` (Tier 1B)

| Milestone | Status | Commit |
|-----------|--------|--------|
| Workspace manifest purity (no tokio, canonical path deps) | Complete | `e22c95f` |
| OMS pure synchronous state machine | Complete | `08112a8` |
| Portfolio FIFO tax-lot on `AccountState` aggregate | Complete | `7c56840` |
| Journal double-entry ledger + `AccountCode` | Complete | `2019ae6` |
| Facade re-exports + repository traits + sizing | Complete | `5b6a051` |

The file-by-file mapping is in [`code-migration-map.md`](code-migration-map.md).

## Subsequent landings

| Crate | Landing | Commit(s) |
|-------|---------|-----------|
| `vyapaar-ledger-allocation` (mean-variance + risk parity, Phase 1) | Complete | `b35a314`, `436a2d8`, `53de244` |
| `vyapaar-ledger-performance` (annotations, metrics, calendar, compare) | Complete | `b7d7fec`, `2a3a04e`, `1b7fb97` |
| Docs: per-crate code references + implementation record | Complete | this change |

## Verification (2026-11-12, `--profile prod`)

```bash
grep -rn "#\[cfg(test)\]" crates/*/src/   # zero inline tests in production source
cargo fmt --check
cargo clippy --workspace --all-targets --profile prod -- -D warnings
cargo test --workspace --profile prod     # 182 tests, all passing
```

- Zero inline `#[cfg(test)]` blocks in `crates/*/src/` (all tests live in
  `tests/` parallel to `src/`).
- Test distribution: facade 4, OMS 25, portfolio 36 (23 + 7 + 6), journal 58
  (18 + 27 + 8 + 5, incl. proptest invariants), allocation 17, performance 42
  (10 + 10 + 12 + 6 + 4). Total **182**.

## Open future work

All open items now live in [`docs/gaps/`](../gaps/README.md) and
[`docs/plans/`](../plans/README.md) — notably the allocation engine Phases 2–4
(Hrp, Black-Litterman, exact bounds, discrete allocation), the performance
journal v2 (temporal-determinism fix, real edge-score formula), and settlement
& statutory-fee consolidation (external NSE calendar, per-lot corporate
actions, per-segment fee tables, canonical golden framework, OMS context
wiring).
