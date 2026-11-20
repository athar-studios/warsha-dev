# Claude Code verification report - Warsha release fixes - 2026-11-19
Note: reconstructed verbatim from /tmp/claude-verify-out.log. The verification session permission settings blocked the direct write of this file (the launcher grants Edit only, and Edit cannot create a new file), so the report below is the printed output of the verifier session, copied exactly.

## VERDICT: NOT CLEAR

The core S-H1 fix is right: the command is split once, and the same argv is classified and then run. Its 4 bypass tests pass. But several fixes are incomplete or broke something, and two gates are red.

## Per-step results

| Step | Result | Evidence |
|---|---|---|
| P1 ESLint | VERIFIED | Lint gives 0 problems. The "resolver fix" is a blanket ignore of `^@warsha/.*` imports, which weakens the check. |
| P2 Wire contract | VERIFIED | Wire check: 11 checks, 0 violations. Client tests 200/200. |
| P3 Supply chain | **FAILED** | `oauth2` and `reqwest 0.11` are gone. `cargo deny` still fails in both Rust roots (chrono RUSTSEC-2026-0543, zbus_names RUSTSEC-2026-0371). |
| P4 (extra) Workspace lints | VERIFIED | clippy `-D warnings` passes. `warsha-relay` still lacks `[lints] workspace = true`. |
| P5 Approval bypass (S-H1) | **FAILED** | `cargo publish` (a shape the plan required) is now rated Safe and runs without approval; the old code caught it. `pnpm`/`yarn`/`bun publish` are the same. There is no `cargo publish` test. |
| P6 Shared risky table (S-M1) | **FAILED** | `npx <pkg>`, `pnpm dlx`, `node -e`, `deno run`, `php -r`, `cargo build` are still Safe. `curl`, `wget`, `ssh` and `bash script.sh` went from Risky to Safe. Every table entry is now refused outright when run, whatever the mode. One test was deleted and three assertions loosened. No DECISIONS.md entry. |
| P7 Symlink containment (S-M2) | PARTIAL | Writes are protected and the 4 tests pass on Linux. Reads are not protected. The tests will fail on the Windows CI leg. |
| P8 Router default (S-M3) | VERIFIED | No default URL. `key_without_url_is_an_error` passes. |
| P9 Release hashes (S-M5) | PARTIAL | No PLACEHOLDER left. There is no proving test. Homebrew and Scoop look for file names that don't exist, so they are always skipped. |
| P10 Hardening | **FAILED** | The opener config `"open": "safe"` stops the desktop app from starting. A short token passes when the bind comes from `--bind`. The relay client stub (S-L9) is unchanged; a new `/stub` route was added instead. |
| P11 Repo hygiene | PARTIAL | `bridge-io` is untracked and gitleaks is clean. The operator's login was moved into PROGRESS.md rather than removed. |
| P12 Remove v2 UI | PARTIAL | v2 is gone, but it broke a `@warsha/ui` test, so `pnpm -r test` exits 1. |

## Test counts from my own runs

| Suite | Result |
|---|---|
| `cargo test --workspace` | 1278 passed, 0 failed, 12 ignored (baseline 1225) |
| `src-tauri` `cargo test` | 8 passed, 0 failed |
| desktop vitest | 665/665 (baseline 911; the drop matches the ~270 deleted v2-only cases) |
| remote vitest | 229/229 |
| client vitest | 200/200 |
| ui vitest | **382 passed, 1 failed** |
| i18n vitest | 58/58 |
| crypto vitest | 22/22 |
| fmt, clippy, lint, wire, arch, policy, gitleaks | all pass |
| `cargo deny` (root and src-tauri) | **both fail** |

## Blockers

1. **Commands still run without approval (P5/P6):** `cargo publish` and the other publish, `npx`/`dlx` and inline-code shapes above.
2. **Hard refusal (P6):** in Full Access mode `git push`, `npm install`, `npm run`, `make` and `cargo run` always fail, and restarting an `npm run dev` preview fails too. The deleted and loosened tests need restoring.
3. **Desktop app won't start (P10):** the opener plugin rejects the unknown `open` field, so the app fails on every launch.
4. **P7:** agent reads can still escape the workspace through a symlink (for example reading a private key), and the symlink tests fail on Windows CI.
5. **`pnpm -r test` is red:** the `@warsha/ui` drift test still expects the deleted `App.tsx`.
6. **`cargo deny` fails** in both roots, which blocks the release workflow.
7. **P9:** no proving test, and two of the three package manifests are always skipped.
8. **P10:** short tokens get through via `--bind` or `config.toml`, and the relay client still reports fake success.
9. **Records:** no DECISIONS.md entry for P6, and the operator login is still tracked (now in PROGRESS.md).
