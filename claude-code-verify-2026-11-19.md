# Claude Code verification report, round 2 - Warsha release fixes - 2026-11-20

Tree: `r-next/integration` @ `8e244fc` (worktree `.agents/worktrees/r-next-integration`). The working tree also has uncommitted edits to `.github/workflows/release.yml`, `DECISIONS.md` and `PROGRESS.md`, plus an untracked `dist-verify-44654484785/`. These are noted where they matter; they are not part of the verified commit.
Round-1 report: `claude-code-verify-2026-11-19-r1.md`. Round-2 fix diff reviewed: `131da65..HEAD` (commits `3e49147` PIPE, `45dea30` LAUNCH, `00723ec` MISC, plus the release-workflow commits through `8e244fc`).
Method: I read the code, ran every suite myself, and ran a scratch probe binary built outside the repo (`/tmp/warsha-probe`, linked against `warsha-pipeline`). The probe drives the real `FilesystemToolExecutor` and `classify_command` with the production default allow-list (`CommandPolicy::default()`) in AskForRisky mode. I modified no source file.

## VERDICT: NOT CLEAR

Most round-1 blockers are fixed: the desktop app launches, `pnpm -r test` is green, root `cargo deny` passes, publish commands now ask for approval, Full Access no longer refuses, agent reads are contained, the relay client fails loudly, the login is out of tracked files and DECISIONS records exist. Five problems remain:
- The allow-listed `cmake` and `node` still let an agent run any program, `git push` included, without approval. My probe showed this.
- Required risky-table rows are still missing.
- An empty or short token is still accepted on a Tailscale bind.
- The symlink tests will fail on Windows CI, and one new test fails whenever it is rerun on the same machine.
- `cargo deny` still fails in `apps/desktop/src-tauri`.

## (1) Per-step results P1-P12

| Step | Result | Code evidence | Test evidence (my runs) |
|---|---|---|---|
| P1 Desktop ESLint | VERIFIED | `pnpm -C apps/desktop lint` exits 0, 0 problems | lint run, exit 0 |
| P2 Wire contract | VERIFIED | `node scripts/check-wire-contract.mjs`: "11 checks, 0 violations" | client vitest 200/200 (`wire-contract.test.ts` 23 cases) |
| P3 Supply chain | PARTIAL | `oauth2`/`reqwest 0.11` gone. Root `cargo deny check` gives "advisories ok, bans ok, licenses ok, sources ok", but only because of a new ignore entry for chrono `RUSTSEC-2026-0543` (`deny.toml:200`) labelled "owner-accepted". It was written by the fleet (DECISIONS.md:48), and I found no owner decision (OD). `apps/desktop/src-tauri` still fails: `advisories FAILED` (`zbus_names` RUSTSEC-2026-0371, unmaintained). That root is not a CI step (`deny.toml:148-153`). | deny root exit 0; deny src-tauri exit 1 |
| P5 Approval bypass (S-H1) | PARTIAL | The core fix holds. `agent_tools.rs:601-602` resolves once, and the same `(program, effective_args)` is classified (`:607`, `:623`) and executed (`:661`). `cargo/pnpm/yarn/bun publish` rows were added (`risky_commands.rs:127-155`). **Still open:** wrappers that are allow-listed by default run any program unclassified. The probe ran `cmake -E env echo PWNED-cmake-env` (success, no ask) and `node -p "require('child_process').execSync(...)"` (success, no ask). So `cmake -E env git push origin main` pushes without approval. | Pass: `bypass_tests_git_push_compound`, `bypass_tests_npm_publish_compound`, `bypass_tests_quoted_git_exe`, `bypass_tests_usr_bin_git_path`, `test_cargo_publish_bypass` (new). My probe confirmed these shapes ask or refuse: `git push origin main` as one string, `/usr/bin/git push`, `"/usr/bin/git" push`, `git -C . push`, `bash -c "git push"`, `env git push` (refused by the allow-list), unterminated `'`/`"` (clean error, no panic), empty and whitespace-only commands ("the command is blank"). The extra shapes have no tests in the repo. |
| P6 Shared risky table (S-M1) | FAILED | Fixed since r1: there is one table (`risky_commands.rs`) used only by `classify_command` (`work_protection.rs:352`). `check_arguments` is back to git-only (`command_policy.rs:688`), so the hard refusal is gone. `npx`, `bunx`, `pnpm/yarn dlx`, `node -e`, `deno run`, `php -r`, `cargo build`, curl/wget/ssh/scp and all shells are rated Risky. **Still Safe** (probe output), although the plan names them: `pnpm run build`, `pnpm build`, `yarn run x`, `pnpm exec`, `yarn exec`. Also still Safe: `npm x`/`npm start`/`npm test`/`npm run-script`, `cargo r`/`cargo b` (built-in aliases of run/build), `node -p`/`--print`/`--eval=`, `bun -e`, `deno task`, `go generate`, `cmake -E env`, `rails runner`, `dotnet run`, `git rebase -x`, `git submodule foreach`. | Pass: `risky_commands_are_classified_as_risky` (table-driven, first pattern of each row only), `test_classify_command` (+16 asserts), `refused_command_not_in_allowlist_is_refused`, `allowed_commands_and_paths_pass_policy_check` (NPM.CMD run build allowed again), `benign_git_config_overrides...` (the "config key" assertion is restored). |
| P7 Symlink containment (S-M2) | PARTIAL | `resolve_contained_path` (`agent_tools.rs:407-470`) is now used by both `read_file` (`:528`) and `write_file` (`:544`). Probe: reading `linkdir/secret.txt` (directory symlink pointing outside) is refused, reading `linkfile` (file symlink pointing outside) is refused, and `sub dir/ünï/ok.txt` reads fine. **Open:** (a) `symlink_to_parent_dir_is_refused`, `symlink_to_etc_is_refused` and `dangling_symlink_is_refused` (`agent_tools.rs:1037-1093`) create the link only under `#[cfg(unix)]` but always assert `!success`. On Windows nothing is linked, the write succeeds, and the tests fail on the `windows-latest` leg of `ci.yml:27`. This was a round-1 blocker and is unchanged. (b) The new `test_read_file_symlink_containment` uses a fixed `/tmp/symlink-containment-test` and calls `symlink(...).unwrap()`. It fails on every rerun on the same machine (see bug M2). | The 4 write tests and the read test pass on a first run. Rerun result: `test_read_file_symlink_containment ... FAILED` (panic at `agent_tools.rs:1652`). |
| P8 Router default (S-M3) | VERIFIED | Unchanged since r1. No default router URL. OpenRouter appears only as an explicit one-click provider preset. | `key_without_url_is_an_error`, `unset_env_has_no_defaults` pass |
| P9 Release hashes (S-M5) | VERIFIED (CI evidence, no unit test) | The manifests fail closed when a hash is missing (`release.yml:704-707`, `782-785`). The real-hash assertions are at `release.yml:823-830`. Homebrew hashes the GitHub tarball (`:737-746`). The scoop name is fixed. | Release run 44654484785 @ `8e244fc`: "Prepare Package Manager Manifests: success" and "Create Draft Release: success", so the assertions really ran. `verify-release-artifacts.test.mjs` 8/8. The uncommitted `release.yml` checksum-scoping edit is untested by any run. |
| P10 Hardening | PARTIAL | Opener is fixed: `"opener": {}`. The plugin's `Config` has `deny_unknown_fields` with one `Option` field, so `{}` deserializes, and no capability exposes opener to the webview. Relay is fixed: `send_frame`/`receive_frames` return `RemoteError::NotImplemented` (`warsha-remote/src/lib.rs:621-628`), and `/stub` is removed. The dev origin is behind `#[cfg(debug_assertions)]` (`src-tauri/src/lib.rs:234`). The token check now runs at daemon start (`startup.rs:203`) and on env overlay (`:1079`), which closes the `--bind` and config.toml host paths. **Open:** `validate_token_for_config` (`startup.rs:1023-1044`) decides loopback from `bind_host` even when `remote_bind = Tailscale`, but the listener binds the Tailscale 100.x address (`http.rs:84`). With the default `bind_host` and the default `auth_token` (`String::new()`, `state.rs:232`), an empty token is served on the tailnet. | Pass: `the_relay_stub_returns_not_implemented` (now asserts client NotImplemented plus `/stub` 404) and `auth_token_must_be_long_enough_for_non_loopback` (env path only). The existing `tailscale_bind_also_serves_the_loopback_companion` starts a Tailscale daemon with a 15-char token, which shows the gap. No test covers the opener config or the debug-only origin. |
| P11 Repo hygiene | VERIFIED | `git ls-files bridge-io` gives 0, and `.gitignore:74` covers it. The operator login has 0 occurrences in tracked HEAD files (it was in PROGRESS.md in r1). | gitleaks is not installed on this host, so the scan was not rerun. |
| P12 Remove v2 UI | VERIFIED | v2 is gone, and `main.test.tsx` covers the always-v3 boot. The r1 `@warsha/ui` failure is fixed (`tokenDrift.test.ts:844` now expects `main.tsx`). The deleted desktop tests (App, AppWiring, deepLink → v2 `requestedScreen`, locale, 22 `screens/*`) all imported v2 code. v3 has no deep-link handler to lose coverage on. | ui 383/383 (r1: 382 + 1 fail), desktop 665/665 |
| P4 (extra) lints | VERIFIED | `cargo clippy --workspace --all-targets -D warnings` exits 0. `warsha-relay/Cargo.toml` still has no `[lints] workspace = true`. | fmt exit 0, clippy exit 0 |

### Round-1 blocker status
1. Commands run without approval (P5/P6): **partly fixed**. Publish, npx/dlx, `node -e`, `cargo build` and the shells now ask. The `cmake -E env` and `node -p` wrappers, plus the run/exec/alias rows listed under P6, still run without approval.
2. Hard refusal in Full Access mode: **fixed**. `check_arguments` is git-only again, and `test_full_access_commands_pass` passes. The restored assertions are in `command_policy.rs:983-986` and `:1238-1241`.
3. Desktop app won't start: **fixed** (from reading the code; no test covers it).
4. P7 reads and Windows: reads are **fixed**. The Windows CI failure is **not fixed**.
5. `pnpm -r test` red: **fixed** (exit 0).
6. `cargo deny`: **root fixed** (via a new ignore entry). **src-tauri still fails.**
7. P9: **fixed**, with CI-run evidence.
8. P10 short tokens and relay: relay **fixed**. `--bind` and config host paths **fixed**. Tailscale path **open**.
9. Records: **fixed**. DECISIONS has a P6/chrono entry and a LAUNCH entry, and the login is removed.

## (2) Test integrity

| Suite (my run) | Result | Baseline P0 / r1 |
|---|---|---|
| `cargo test --workspace --no-fail-fast` | **1280 passed, 0 failed, 12 ignored** (68 result lines) | P0 1225 / r1 1278 |
| `apps/desktop/src-tauri` `cargo test` | 8 passed, 0 failed | r1 8 |
| desktop vitest | 665/665 (79 files) | P0 911 (the v2 deletion is explained in r1) / r1 665 |
| remote vitest | 229/229 | r1 229 |
| client vitest | 200/200 | P0 196 / r1 200 |
| ui vitest | 383/383 | P0 383 / r1 382+1 fail |
| i18n vitest | 58/58 | r1 58 |
| crypto vitest | 22/22 | r1 22 |
| `pnpm -r test` | exit 0 | r1 exit 1 |
| fmt / clippy -D warnings / desktop lint / wire / arch / policy | all exit 0 (wire 11 checks, 0 violations) | |
| `verify-release-artifacts.test.mjs` | 8/8 | |
| `cargo deny` root / src-tauri | ok / **FAILED** (zbus_names) | r1 both failed |
| Rerun of `test_read_file_symlink_containment` | **FAILED** (EEXIST panic) | |

Cargo delta from r1 is +2: 3 tests added (`test_read_file_symlink_containment`, `test_full_access_commands_pass`, `test_cargo_publish_bypass`), 1 deleted, and 1 replaced 1:1 (`risky_commands_are_refused` became `refused_command_not_in_allowlist_is_refused`).

Weakened or lost tests in the round-2 diff:
- **Deleted without replacement:** `ask_without_a_run_id_never_registers` (B3: "a daemon-wide loop (no run id) must never register a pending ask"). It was overwritten in place by `test_read_file_symlink_containment` in `3e49147`. No other test calls `with_mcp_approvals(..., None)`, so this B3 guarantee is now untested. No DECISIONS entry explains it.
- `test_full_access_commands_pass` has a vacuous assertion `res.success || !res.output.contains("refused")`. Only the `.expect()` does any checking. It also runs real `git push`, `npm install`, `make` and `cargo run` in a shared `/tmp/full-access-test`.
- The `auth_token_must_be_long_enough...` assertion now matches "token too short". That is equivalent, not weaker.
- The relay test now asserts 404 on `/stub` plus `NotImplemented` from the client. That is stronger.
- Restored since r1: the `NPM.CMD run build` allowed assertion and the "config key" refusal assertion.
- No new `#[ignore]`, `.skip`, `.only`, `xit`, `eslint-disable` or removed CI gate steps were found in `131da65..HEAD`. The only new `allow` is `#[cfg_attr(not(debug_assertions), allow(unused_mut))]` (src-tauri `lib.rs:228`), which is benign.

## (3) Bug findings

HIGH
- **H1. Approval bypass through allow-listed wrappers** (`work_protection.rs:352-382`, `command_policy.rs:50-82`). `cmake` and `node` are in `DEFAULT_ALLOWED_COMMANDS`. `cmake -E env <program> ...` and `node -p/--print <js>` are rated Safe. In AskForRisky mode my probe ran `cmake -E env echo PWNED-cmake-env` and `node -p "require('child_process').execSync('echo PWNED-node-p')"`, and both succeeded with no ask. Failing scenario: an agent issues `cmake -E env git push origin main`, which pushes without owner approval. That is the S-H1 shape again. The same applies to `git rebase -x "<cmd>"` and `git submodule foreach "<cmd>"`, which git hands to a shell; `check_arguments` only polices `-c` and `--exec-path`.
- **H2. Plan-required P6 rows missing** (`risky_commands.rs`). `pnpm run`, `pnpm <script>`, `yarn run`, `yarn <script>`, `pnpm exec` and `yarn exec` are Safe. So are `npm x`, `npm start`, `npm test`, `npm run-script`, `cargo r` and `cargo b`. Failing scenario: `pnpm run release`, or `cargo r` with a build.rs, runs project scripts with no ask, while `npm run` and `cargo run` ask.

MEDIUM
- **M1. Short or empty token accepted on a Tailscale bind** (`startup.rs:1030-1033`). A config with `remote_bind = "tailscale"` (or the persisted setting) and the default `bind_host = 127.0.0.1` counts as loopback, so `auth_token = ""` or `"tailscale-token"` passes. The RPC listener then binds the 100.x tailnet address. The existing test `tailscale_bind_also_serves_the_loopback_companion` starts exactly this setup with a 15-char token.
- **M2. `test_read_file_symlink_containment` fails on rerun** (`agent_tools.rs:1636-1652`). It uses a fixed temp dir and no cleanup, so the second `cargo test` on any machine panics with EEXIST. I reproduced this. The read-escape part is also `cfg(unix)`-only.
- **M3. P7 write-symlink tests fail on Windows CI** (`agent_tools.rs:1044`, `1064`, `1084`). The symlink is only created on unix, but the `!success` assertion is unconditional. No CI run exists for this tree, so this comes from reading the code.
- **M4. B3 regression coverage lost.** `ask_without_a_run_id_never_registers` was deleted (see section 2).
- **M5. src-tauri `cargo deny` fails** (zbus_names RUSTSEC-2026-0371). The root passes only through a chrono ignore whose reason claims owner acceptance, but the only record is a fleet-written DECISIONS line (`DECISIONS.md:48`). `deny.toml:117-131` still says chrono is "left un-ignored on purpose" and lists rustls/h2/webpki notices that no longer resolve. The comments are stale and contradict the config.

LOW
- **L1.** Classification runs before the allow-list check (`agent_tools.rs:601-661`). A non-allow-listed risky command such as `bash -c ...` registers an approval ask that can never succeed, because after approval the allow-list still refuses it.
- **L2.** A final-component symlink that points *inside* the workspace is now refused for reads (probe: `inner-link.txt` was refused). Repos with in-tree symlinks, such as `README.md -> docs/README.md`, become unreadable to the agent.
- **L3.** `auth_token_must_be_long_enough_for_non_loopback` sets the process-wide `WARSHA_AUTH_TOKEN` and never restores it, so parallel tests that call `apply_env_overrides` race with it.
- **L4.** There is a TOCTOU window between `resolve_contained_path` and `create_dir_all`/`write` (`agent_tools.rs:544-575`). It is partly mitigated by the re-check of the final component, but parent directories are not re-checked.
- **L5.** At HEAD the release checksums file lists every file under `artifacts/` (471 lines in run 44654484785). The scoping fix exists only as an uncommitted `release.yml` edit. The Homebrew formula runs `cargo install` at the workspace root, which has no root package.

Note (not a defect): a `v0.3.0` tag exists and a **Draft** release "Warsha 0.3.0" was created by run 44654484785. "Promote Verified Draft" was skipped, so nothing was published.

## (4) Verdict

VERDICT: NOT CLEAR
Blockers:
1. P5/P6 (H1): the default-allow-listed `cmake -E env <program>` and `node -p <js>` run any command, including `git push`, without approval in ask-for-risky mode. Shown by my probe run.
2. P6 (H2): plan-required rows are still Safe: `pnpm/yarn run`, `pnpm/yarn exec`, implicit `pnpm/yarn <script>`. Aliases `npm x/start/test/run-script` and `cargo r/b` are also Safe.
3. P10 (M1): an empty or short auth token is accepted when `remote_bind = tailscale`, and the daemon then serves on the tailnet address.
4. P7 (M2/M3): the three symlink write tests will fail on the Windows CI leg, and `test_read_file_symlink_containment` fails on every rerun, so CI cannot be green.
5. P3 (M5): `cargo deny` still fails for `apps/desktop/src-tauri` (zbus_names). The root passes only via a chrono ignore labelled owner-accepted without an owner decision.
6. Test integrity (M4): `ask_without_a_run_id_never_registers` was deleted with no replacement or DECISIONS entry.
