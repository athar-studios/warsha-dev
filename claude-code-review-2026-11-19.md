# Warsha pre-release review: `r-next/integration` @ `3c8b3a1`

**Verdict: not safe to release as-is.** There is one HIGH finding: the agent can run a push or publish without the owner's approval. Two CI gates I ran fail (desktop ESLint and the wire contract), and the project's own final report says `cargo deny` fails too. Once steps P1–P9 below are done and the full gate run passes, the tree is reasonable to ship as the documented early preview (v0.3.x).

I made no edits to the repo. I ran grep and git-history scans, `cargo fmt --check` on both Rust workspaces (passes), `node scripts/check-arch.mjs` (0 violations), `node scripts/check-wire-contract.mjs` (4 violations), and `npx eslint --no-cache apps/desktop/src` (18 problems). I did not run `cargo clippy`, `cargo test`, `cargo deny` or `pnpm test`, because they write build output; step P0 records them. The `cargo deny` failure comes from the R-NEXT final report (risk R1). The same text is saved at `/home/moe/.claude/plans/you-are-doing-an-snoopy-rain.md`.

---

## 1. Security

### HIGH

**S-H1 — The approval check on agent commands can be bypassed.**
- **The bug:** the risk check and the executor look at different things.
  - `crates/warsha-pipeline/src/agent_tools.rs:526,542` calls `classify_command(command, args)` on the agent's raw `command` string.
  - `command_policy.rs:646-666` (`resolve_program_and_args`) then splits that string again before running it.
  - `work_protection.rs:350-385` only matches exact names like `program == "git"`.
- **Bypass 1:** `{command:"git push origin main", args:[]}` is rated Safe and then runs `git push origin main`. `"npm publish"` and `"cargo publish"` work the same way.
- **Bypass 2:** `{command:"/usr/bin/git", args:["push"]}` is rated Safe. The allow-list still accepts it because it matches on the file name alone (`command_policy.rs:453-456`, `program_name` at :804).
- **Why it hits a default install:**
  - The default mode is "ask for risky" (`work_protection.rs:74-76`).
  - An empty `allowed_commands` (`crates/warshad/src/state.rs:239`) falls back to the default list (`command_policy.rs:328-333`), which includes `git`, `npm` and `cargo`.
  - The tool call is model output, so a prompt injection controls it.
- **Impact:** a push or publish the owner never approved. `env_clear()` does not stop SSH, which finds keys through the user account rather than `$HOME`.

### MEDIUM

- **S-M1 — Commands that run arbitrary code count as "Safe".**
  - `work_protection.rs:350-385` passes all of these without asking: `node -e`, `npm run <script>`, `make`, `cargo run`/`build` (runs `build.rs`), `npx`, `pnpm dlx`, `bun x`, `deno run`, `php -r`, `pnpm/yarn/bun add|install`, `npm i`, `npm ci`.
  - `pnpm`, `yarn` and `bun` do not appear in the classifier at all.
  - They run with the user's rights and network access, and there is no OS sandbox.
  - The code's own safety argument at `command_policy.rs:796-800` ("unacceptable if a tool can create an executable bit") is already false: `node`, `npm` and `git` can all create executables.
- **S-M2 — Agent file tools can escape the workspace through symlinks.**
  - `FilesystemToolExecutor::resolve` (`agent_tools.rs:380-397`) only checks the path text, and the reads and writes then follow symlinks.
  - Example: a symlink brought in by an attached repo, a `git clone` or `node -e` lets the agent write `link/.bashrc`.
  - The daemon's static file handler already does this correctly (`crates/warshad/src/http.rs:570-604`).
- **S-M3 — Prompts go to a third-party router by default.**
  - `crates/warsha-providers/src/env_llm.rs:14` defaults to `https://router.sayl.site/v1`.
  - The Docker files set the same default: `docker/entrypoint.sh:47-48` and `deploy/.env.example:24,32`.
  - Setting a key without a base URL sends the key and all project content to that service.
  - This path also reads its key from an environment variable, which departs from AGENTS.md's "secrets only via OS keychain".
- **S-M4 — A dependency that is never used breaks `cargo deny`.**
  - `crates/warsha-providers/Cargo.toml:17` declares `oauth2 = "4.4.2"`, and no source file imports it.
  - Per `Cargo.lock`, it pulls in `reqwest 0.11.27`, which brings old `rustls`/`rustls-pemfile`. Those carry RUSTSEC-2025-0134 and related notices (final report R1 / `D-RN-A5-1`).
  - `keyring = "1.2"` is a 2021-era crate behind 4 of the 6 ignores in `deny.toml`.
  - RUSTSEC-2026-0459 (bundled SQLite) is an owner-accepted risk, not a fix.
- **S-M5 — Package-manager files ship with placeholder hashes.** `.github/workflows/release.yml:696,718,747` write `PLACEHOLDER_SHA256` (and `ProductCode "{PLACEHOLDER}"`) into the winget, Homebrew and Scoop files.
- **S-M6 — Weak spots when the daemon is reachable over a network.**
  - On a non-loopback bind, bearer tokens travel over plain HTTP (`startup.rs:644`; documented in README:162).
  - There is no minimum token length (`startup.rs:1050`), even though Docker binds `0.0.0.0` (`docker/Dockerfile:116`).
  - Behind Caddy, every caller shares one rate-limit bucket, because the limiter keys on the connecting address (`http.rs:421`).

### LOW

- **S-L1:** `tauri-plugin-opener` is registered (`src-tauri/src/lib.rs:941`, `Cargo.toml:35`, `tauri.conf.json:70-71`) but no permission grants it, and `Cargo.toml:28` plus DECISIONS say no plugin exists. It is unused attack surface that could be granted by accident later.
- **S-L2:** The desktop CSP allows connections to any loopback port: `connect-src … 127.0.0.1:* localhost:*` (`tauri.conf.json:31`).
- **S-L3:** The Vite dev address `http://localhost:5173` is trusted in release builds (`src-tauri/src/lib.rs:227-238`; `docker/entrypoint.sh:78,85`).
- **S-L4:** The daemon's CSP (`http.rs:295`, copied in `apps/remote/index.html:59`) still allows inline styles (`unsafe-inline`), and its `connect-src` pairs `http`/`ws` with the wrong ports. Ports 8765 and 8766 are HTTP/events and JSON-RPC respectively (`state.rs:228-229`).
- **S-L5:** Pairing secrets are stored as plain text (`warsha-store/migrations/006_settings.sql:48`, compared at `auth.rs:213`).
- **S-L6:** `macOSPrivateApi: true` (`tauri.conf.json:33`) isn't needed and blocks the Mac App Store.
- **S-L7:** Builds are unsigned and there is no auto-updater (`tauri.conf.json:38`; documented decision). `release.yml:44-46` still mentions an updater endpoint at "tauri.conf.json:74", which doesn't exist.
- **S-L8:** `bridge-io/` is listed in `.gitignore:74` but 71 of its files were force-added. The logs include local home-folder paths (46 tracked files contain them in total). `COMPLETION_STATUS.md:3` wrongly says the folder is ignored, and `DECISIONS.md:933` names the operator's GitHub login.
- **S-L9:** `RelayClient::send_frame` and `receive_frames` (`warsha-remote/src/lib.rs:619-626`) report success without doing anything.

### Checked and clean

- **Secrets:** nothing live in the tree or the git history. The only key-like strings are test fixtures.
- **Daemon auth:**
  - The token is 32 bytes from the OS random source, written to a 0600 file.
  - Comparisons are constant-time (`warsha-remote/src/lib.rs:68`), and an empty token is refused.
  - Only `/health` answers without a token.
- **Authorization table:** 132 methods (`auth.rs:264+`) with a coverage test. Secrets, MCP, pairing and provider methods are owner-only.
- **Static file serving** and **outbound MCP/registry HTTP:** traversal-safe, with redirects re-checked and response sizes capped.
- **Frontend:** no XSS sinks, the remote app keeps its token in `sessionStorage`, and WebMCP only exposes navigation.
- **CI and release:** read-only permissions, draft-only releases with a fail-closed promote step, and a pinned `cargo-deny`.
- **Docker:** runs as a non-root user, with security headers from Caddy.

## 2. Code cleanliness

- **C1 — Desktop ESLint fails (18 problems; CI runs `pnpm lint` at `ci.yml:430`).**
  - `as any` at `apps/desktop/src/v3/pages/project/index.tsx:1064,1074`, which breaks AGENTS.md's "no any".
  - 14 unresolved imports of `@warsha/i18n/v3` and `@warsha/ui/v3`.
  - 1 import-order error in `frame/QuotaGaugeButton.tsx:7`.
  - 1 hooks warning in `inbox/index.tsx:336`.
- **C2 — Wire contract fails (4 violations).** `catalog.refresh` and `catalog.apply` (`packages/client/src/index.ts:3439,3443`) exist in the client but the daemon doesn't register them, and the e2e harness has no stubs.
- **C3 — The workspace lint rules never apply.** `[workspace.lints]` (`Cargo.toml:85-97`, including deny on `unwrap`/`expect`/`panic` and unused-dependency warnings) is never switched on: no crate has `[lints] workspace = true`. That is why the unused `oauth2` went unnoticed. The no-`unwrap` rule holds only by habit; one non-test `expect` remains (`warsha-backends/src/catalog.rs:229`).
- **C4 — Dead code.**
  - The `warsha-catalog` crate has no dependents.
  - `oauth::{device,pkce}` is exported (`warsha-providers/src/lib.rs:18`) but never called.
  - The `RelayClient` stub.
  - `#[allow(dead_code)]` at `warsha-backends/src/lib.rs:18-31`, `warsha-pipeline/src/lib.rs:419,2135`, `warshad/src/api.rs:919` and `wave2.rs:217`.
- **C5 — The old v2 UI still ships.** It is reachable via `?ui=v2` (`apps/desktop/src/main.tsx:46-64`): 42 legacy screens, and all 7 TODOs live there. Examples: a deploy button that only "reports intent" (`AdvancedMode.tsx:1776`) and an artifact viewer that shows fixture data (`ArtifactViewer.tsx:63`).
- **C6 — Oversized files.**
  - Rust: `warshad/src/tests/mod.rs` (6,961 lines), `warshad/src/lib.rs` (4,122), `warsha-pipeline/src/lib.rs` (3,653), `process_spawn.rs` (3,170).
  - TypeScript: `packages/client/src/index.ts` (4,596), `packages/i18n/src/index.tsx` (2,754).
- **C7 — Confusing names and stale docs.**
  - `ws_port` actually serves JSON-RPC and `http_port` serves the `/events` WebSocket (`state.rs:145-148`).
  - `deploy/docker-compose.yml:16` pins image `0.1.0` instead of `0.3.0`.
  - `ARCH_AUDIT.md` covers 20K lines; the tree now has about 98K lines of Rust.
  - `COMPLETION_STATUS.md` contradicts the final report.
- **Clean:** `cargo fmt` passes and there is no stray `console.log`.

## 3. Architecture

- **The documented rules hold:** `check-arch` reports 5 rules and 0 violations, and every crate dependency follows the layer order.
- **A1 — `warsha-pipeline` is a 40K-line crate doing too much.** It contains the security boundary for running commands (process spawning, command policy, risk classification, MCP stdio) alongside the prompt logic. That is how the classifier and the executor drifted apart (S-H1).
- **A2 — `warshad` is linked into both the CLI and the desktop app.** That is allowed, but `lib.rs` (4,122 lines) has become a merge hotspot.
- **A3 — There are two lists of risky commands:** `command_policy.rs:83` and `work_protection.rs:352`. Neither matches the default allow-list.
- **A4 — The governance gates aren't enforced.** The lint rules are inert and ESLint and the wire contract fail, while the final report calls everything green and AGENTS.md requires green CI.
- **Release risk is moderate.** The structure is sound; the risk sits in command execution and in the failing gates.

## 4. Suggestions

**(a) Must fix before release**
1. S-H1: classify the command as it will actually run.
2. S-M2: keep agent file reads and writes inside the workspace, refusing symlink escapes.
3. S-M1: mark code-running commands as risky.
4. S-M4: drop `oauth2` and get `cargo deny` green.
5. C1 and C2: fix ESLint and the wire contract.
6. S-M5: put real hashes in the package-manager files.
7. S-M3: remove the third-party router default.

**(b) Should fix before release**
- C3: switch on the workspace lints.
- S-M6: require a minimum token length on network binds and fix rate limiting behind the proxy.
- S-L1: remove the opener plugin or grant it explicitly.
- S-L3: trust the dev origin in debug builds only.
- S-L8: stop tracking `bridge-io/`.
- S-L9: make the relay stub return an error.
- C5: remove or hide the v2 UI.
- C7: fix the compose image tag and the stale comment.

**(c) After release**
- OS-level sandboxing for agent commands, with network off by default.
- Move command execution into its own crate (A1).
- Split the oversized files.
- Hash pairing secrets.
- Upgrade `keyring` to 3.x.
- Tighten the CSP to the real ports.
- Code signing plus a Tauri updater.
- Rename `ws_port`/`http_port` to `rpc_port`/`events_port`.
- Refresh `ARCH_AUDIT.md`.

## 5. Fix and update plan for ZadLoop

**The full gate set (G-ALL):**
- `cargo fmt --check`, `cargo clippy -D warnings` and `cargo test` in both the root workspace and `apps/desktop/src-tauri`
- `cargo deny` in both roots
- `pnpm lint && pnpm -r typecheck && pnpm -r test`
- `check-arch`, `check-wire-contract` and `pnpm policy`

Each step is one small Conventional Commit, made in its own worktree under ENGINEERING_POLICY.md, and merged only when its gate passes.

| # | Step | Files and scope | Done means | Gate |
|---|---|---|---|---|
| P0 | Baseline | none | G-ALL output recorded. Expected failures: ESLint 18, wire 4, `cargo deny` advisories | G-ALL |
| P1 | Fix ESLint | `v3/pages/project/index.tsx:1064,1074` (use the typed value instead of `as any`); configure ESLint's import resolver for the `@warsha/*/v3` subpaths; `QuotaGaugeButton.tsx:7`; `inbox/index.tsx:336` | `pnpm -C apps/desktop lint` finds 0 problems | pnpm lint, typecheck, test |
| P2 | Fix the wire contract | `packages/client/src/index.ts:3439,3443`: remove the two methods. Or register them in `warshad` as owner-only and add stubs to `apps/desktop/e2e/harness.ts` | 0 violations | wire check, `pnpm -r test`, `cargo test -p warshad` |
| P3 | Supply chain | Remove `oauth2` from `warsha-providers/Cargo.toml:17`. Upgrade any remaining advisory, or record it in `deny.toml` with an owner-decision ID and end condition, plus a DECISIONS.md entry logged as OPEN | `reqwest 0.11` gone from the tree; `cargo deny` passes for both roots | Rust gates + deny |
| P4 | Switch on lint rules | Add `[lints] workspace = true` to all 11 crates. Fix `catalog.rs:229` and the unused dependencies; clean up or justify the C4 `allow`s. Tests may opt out of the unwrap/expect lints | clippy passes with the rules on | clippy + test |
| P5 | **S-H1** | `agent_tools.rs:517-580`: split the command once, classify the normalized program name with the real arguments, and run exactly that. Normalize the name in `work_protection.rs:350`. Tests: `"git push origin main"`, `"/usr/bin/git"`+`push`, `"npm publish"` and a quoted `.exe` path all require approval | Tests fail on the old code and pass on the fix; every bypass shape queues an approval | `cargo test -p warsha-pipeline -p warshad` |
| P6 | **S-M1** | One shared table of risky commands, used by both `command_policy.rs` and `work_protection.rs`. Risky: inline-eval flags; `npx`/`dlx`/`exec`/`x`; every install alias (`install`, `i`, `ci`, `add`, `update`) for npm, pnpm, yarn, bun, cargo, go, composer, bundle and pip; `run` scripts; `make`/`just`/`task`; `cargo run`; `go run`/`get`; git network verbs; arguments with `~`, a drive letter or an absolute `--prefix`. Record the change in DECISIONS.md | One test per table row; `gate_evidence.rs` tests still pass | pipeline + warshad tests, desktop approval e2e |
| P7 | **S-M2** | `agent_tools.rs:380-489`: resolve the real path of the workspace and target and require containment. For writes, check the deepest existing parent and refuse a final symlink. Reuse the approach in `http.rs:570-604` | Symlinks to the parent folder, to `/etc` and dangling links are refused; normal nested paths still work | pipeline tests (Unix; also check the Windows build compiles) |
| P8 | **S-M3** | `env_llm.rs:14`: no default URL; a key without a URL is an error. Clear the defaults in `docker/entrypoint.sh:47-48` and `deploy/.env.example:24,32`. Update both READMEs (line 171) | No `sayl.site` in current files; a test covers key-without-URL | providers + pipeline tests |
| P9 | **S-M5** | `release.yml` ~680-760: take hashes from `checksums.sha256`, or skip the manifests until real hashes exist. Fix the comment at :44-46 | No `PLACEHOLDER` in generated output (enforced by an assertion) | actionlint, `verify-release-artifacts` tests |
| P10 | Hardening | Require ≥32-character tokens on network binds (`startup.rs`). Trust `localhost:5173` only in debug builds (`src-tauri/lib.rs:227-238`, `docker/entrypoint.sh`). Remove the opener plugin or scope its permission. Make the relay stub return an error. Set the compose image tag to `0.3.0` | Each item has a test; `--self-check` still passes 11/11 | G-ALL |
| P11 | Repo hygiene | `git rm -r --cached bridge-io/`. Remove the operator's login from DECISIONS.md:933 if the repo goes public. Update COMPLETION_STATUS.md | `git ls-files bridge-io` is empty | policy check, gitleaks |
| P12 | Remove the v2 UI (B4b) | Delete `App.tsx`, `screens/**` and the `?ui=v2` switch in `main.tsx:31-64`, or hide them behind a dev flag | No `TODO(M20` left in shipped code; v3 e2e and visual tests pass | `pnpm -r test`, Playwright |
| P13 | Final check | G-ALL, `--self-check`, gitleaks over full history, and an independent reviewer re-reads the P5–P8 changes | Every gate passes, with evidence recorded in PROGRESS.md and DECISIONS.md | G-ALL |

## 6. Release-readiness verdict

**Not safe to release now.** The approval check on agent commands can be bypassed (S-H1) and is too permissive (S-M1). File tools can escape the workspace through symlinks (S-M2). ESLint and the wire contract fail when I run them, and `cargo deny` fails per the project's final report.

**After P1–P9 and a passing P13,** the tree is reasonable to release as the documented early preview (v0.3.x). The release notes should still state these remaining risks:
- builds are unsigned and there is no auto-update;
- approved or "safe" agent commands still run without a sandbox, with the user's rights;
- RUSTSEC-2026-0459 (bundled SQLite) is an accepted risk;
- the network mode uses plain HTTP.

P10–P12 should be in before any public (non-draft) promotion. The architecture rules hold, and auth, static file serving and outbound HTTP are solid.
