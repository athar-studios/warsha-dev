# F3 Live Proof — Attempt A (`f3-live-a`)

**Verdict: PASS**

All four `proofcalc` modules (`src/scaffold.rs`, `src/domain.rs`, `src/engine.rs`, `src/provider.rs`) were written by the live OpenCode backend (`opencode` v2.4.1, model `opencode/flint-40b-free`) inside one complete `warshad` swarm run (`01M8AWJEG42HCPWQ4NKRFFQ6BT`) with `max_parallel = 4`, captured in four persisted `swarm_coding_result` events with non-empty `files_changed` and diffs, merged onto branch `warsha/run-01M8AWJEG42HCPWQ4NKRFFQ6BT`, and verified with `cargo test --offline` (`1 passed; 0 failed`) in an isolated temp directory outside the workspace.

## Environment & Isolation

- Base worktree / commit: `.agents/worktrees/f3-live-a` (`r-next/f3-live-a` at `6ca306e`)
- Isolated runtime root: `/home/moe/tools/f3-live-a/` (`HOME`, `XDG_*`, `data_dir`, `config.toml`, `warshad.sock`)
- `CARGO_TARGET_DIR`: `/home/moe/tools/f3-live-a/target` (for `warshad`) and `/home/moe/tools/f3-live-a/proofcalc-target` (for `cargo test --offline`)
- Ports: `19200` (HTTP), `19201` (WebSocket JSON-RPC), `19202` (private OpenCode `serve` during run)
- Daemon flags/env: `WARSHA_E2E_REAL=1`, `WARSHA_CODING_BACKEND=opencode`, `WARSHA_STAGE_TIMEOUT_SECS=900` (attempt 2; see Attempts below), `fake` provider for planning/gate stages
- OpenCode CLI: `/home/moe/.opencode/bin/opencode` (`2.4.1`), configured with `model: "opencode/flint-40b-free"` and `permission: "allow"` in `/home/moe/tools/f3-live-a/home/.config/opencode/opencode.json`

## Attempts & Retries

1. **Attempt 1 (`01M8AW29BS7Y7ZGWP6QF0F98VH`) — failed at 182s watchdog:**
   - Launched with default `WARSHA_STAGE_TIMEOUT_SECS` (`180`).
   - All 4 parallel OpenCode subtasks actually succeeded and merged onto `warsha/run-01M8AW29BS7Y7ZGWP6QF0F98VH` at ~316s (`02:53:49Z`), and emitted 4 `swarm_coding_result` events, but the stage watchdog had already marked the run `failed` at `02:51:35Z` (`Stage code timed out after 182s (watchdog)`).
   - During attempt 1, OpenCode also installed a stable `rustup` toolchain into `/home/moe/tools/f3-live-a/home/.rustup` because `env -i` omitted `RUSTUP_HOME`/`CARGO_HOME`.
   - Evidence preserved in `attempt1/`.

2. **Attempt 2 (`01M8AWJEG42HCPWQ4NKRFFQ6BT`) — completed cleanly (`feedback completed`):**
   - Relaunched `warshad` with `WARSHA_STAGE_TIMEOUT_SECS=900`.
   - Zero HTTP 429 errors, zero model fallbacks, zero subtask retries.
   - `idea -> gate_a (approved) -> gate_b (approved) -> code (4 parallel subtasks, ~177s) -> gate_c (approved) -> gate_d (approved) -> feedback (completed)`.

## Model Identity & Provenance

- `warshad` invokes `opencode run --format json` without pinning `--model`, so the configured default `opencode/flint-40b-free` was used.
- `opencode.db` `session_v2.model` is `NULL`, while `opencode.db` `session_message` records `"providerID":"opencode","model":{"id":"flint-40b-free","providerID":"opencode"}` on every assistant turn across all 4 sessions (`attempt2/opencode-session-models.json`):
  - `ses_dea36a440ffenkS5cRdZjxjj3r` (`src/scaffold.rs`)
  - `ses_dea369901ffeCWdh7T06NtVUaL` (`src/domain.rs`)
  - `ses_dea369932ffe5kvcYxOgFCAVqY` (`src/engine.rs`)
  - `ses_dea36ad84ffeg27IArF07YZDYj` (`src/provider.rs`)

## Capture Proof (`swarm_coding_result` Events)

All 4 `swarm_coding_result` events in `attempt2/events.json` (extracted to `attempt2/swarm-coding-results.json`) have `backend: "opencode"`, `success: true`, `diff_truncated: false`, non-empty `files_changed`, and full unified diffs:

| Subtask ID | Event ID | `files_changed` | Diff bytes | Commit on `warsha/run-01M8AWJEG42HCPWQ4NKRFFQ6BT` |
| --- | --- | --- | --- | --- |
| `01M8AWJJPMZRQ35E99JEX3EGPA` | `01M8AWNBYSPZY46C6WPG58E37F` | `["src/scaffold.rs"]` | 319 | `b977c63` (merged `64e9241`) |
| `01M8AWJJPMRGTA0B49TQSZZ7FM` | `01M8AWNT9PF012TY248AP5TDJN` | `["src/domain.rs"]` | 368 | `d55375d` (merged `916a689`) |
| `01M8AWJJPMW9TPWXAA17HGM8GH` | `01M8AWQYNRNK55XJ6B4CB6ZAQC` | `["src/engine.rs"]` | 526 | `ebec4d9` (merged `1adab3b`) |
| `01M8AWJJPM07CPMAE6Z9XXBCJ6` | `01M8AWN6CNFT87BE07QKMEY37G` | `["src/provider.rs"]` | 202 | `5e710a1` (merged `3f7e6f1`) |

## Run Branch Files & SHA-256 (`warsha/run-01M8AWJEG42HCPWQ4NKRFFQ6BT`)

| File | SHA-256 |
| --- | --- |
| `src/scaffold.rs` | `4cab5d424b4983f4a7466413e3eac6435ed029ee1240bf829cd3349889b342ab` |
| `src/domain.rs` | `fa34146a1afddce1c85a8a84cbc66441ceaf0f918a1ac68ea8543ef172d40eea` |
| `src/engine.rs` | `415dde023dff7c733e2d70b6d2174c7f754cb06e766a376c75306796d4c56454` |
| `src/provider.rs` | `3ca8afaa43a997f68ed4f7606333d0db9198b434790e769156b388628bb1ce49` |

Full `git show` outputs are in `attempt2/git-show-*.rs.txt` and `attempt2/git-diff-full.patch`.

## Assembled `proofcalc` & `cargo test --offline`

Per `docs/proof-project/SPEC.md` §2, §4.5, and §4.6, the four OpenCode-written files from `warsha/run-01M8AWJEG42HCPWQ4NKRFFQ6BT` were exported to `/tmp/proofcalc-f3-live-a.wOtbfU`, combined with `Cargo.toml`, `src/lib.rs`, and `tests/proof.rs` authored by the assistant (copies archived in `sequential/`), and tested offline:

- Command: `CARGO_TARGET_DIR=/home/moe/tools/f3-live-a/proofcalc-target cargo test --offline`
- Exit code: `0`
- Result: `test four_public_contracts ... ok` (`1 passed; 0 failed`) — see `proofcalc-cargo-test.log`.

## What This Proved & What It Did Not

- **Proved:** End-to-end `warshad` orchestration with `WARSHA_CODING_BACKEND=opencode`, `max_parallel = 4`, real OpenCode (`flint-40b-free`) writing all four `proofcalc` modules in isolated git worktrees, `swarm_coding_result` persistence with non-empty `files_changed` and diffs, branch merge onto `warsha/run-01M8AWJEG42HCPWQ4NKRFFQ6BT`, gate progression through `feedback completed`, and passing `cargo test --offline` on the assembled crate.
- **Not proved / caveats:**
  - Planning/gate roles used the built-in `fake` provider (no paid LLM APIs or keychain secrets).
  - Default `WARSHA_STAGE_TIMEOUT_SECS=180` was insufficient for the first cold run (attempt 1 took ~316s and hit the 182s watchdog); attempt 2 required `WARSHA_STAGE_TIMEOUT_SECS=900` (and took ~177s in `code`).
  - OpenCode wrote scratch compile/test artifacts outside the project worktree under `/tmp/opencode/` (`provider_check.rlib`, `fee_check/*`, and transient `money_check`, `domain_check.rlib`, `scaffold_check.rlib` that were cleaned up); inside the git worktrees, only the assigned `src/<lane>.rs` files were written (`attempt2/opencode-tool-calls.txt`).
  - Edge case noted in OpenCode-generated `src/engine.rs`: `let signed_fee = if amount < 0 { -(fee as i64) } else { fee as i64 };` would overflow-panic in debug builds for `apply_fee(i64::MIN, 10_000)` (`-(i64::MIN)`), though `cargo test --offline` against the SPEC §4.6 cases passes cleanly.
