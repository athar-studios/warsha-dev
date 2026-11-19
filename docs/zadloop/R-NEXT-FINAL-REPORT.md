# R-NEXT-FINAL-REPORT — Z1 release report

> **DRAFT — SKELETON ONLY. NOT A FINAL REPORT.** Structure and evidence index only; every result
> marked `TBD` is filled by the Z1 run after E5 and A5 merge. No number in this file is a Z1
> measurement.

| Field          | Value                                                                                                                                                       |
| -------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status         | DRAFT skeleton (branch `rn-z1`, worktree `.agents/worktrees/rn-z1`)                                                                                         |
| Base commit    | `11104c8` (`r-next/integration`; tip is `fix(e2): avoid object spread in secretary client RPCs for wire contract`)                                          |
| Pending merges | **E5** sweep — `r-next/e5-e1-cleanup` @ `f7ab39f`; **A5** audit — `r-next/a5-health` @ `978e40e` (neither is an ancestor of `11104c8`)                      |
| Version        | stays `0.3.0` — this report only _recommends_ (§5)                                                                                                          |
| `main`         | unchanged from the start of the run (re-verify at Z1: expected `939a173ff94d6de5aeb6b4827c57143b247d09e1`, matching `docs/zadloop/packets/FINAL_REPORT.md`) |
| Spec           | `docs/zadloop/R-NEXT-MASTER-PLAN.md` §10 (Z1), §3.1 (acceptance), §3.4 (packet report), A11 (Z1 = release report)                                           |

**Rule for filling this file (§10 acceptance):** every claim cites an evidence file or a commit.
A row with no evidence path stays `TBD`. Gate numbers come from Z1 gate logs, not from memory or
from the "prior recorded" column below.

---

## 1. Per-packet table

Merged = ancestor of `11104c8` (checked with `git merge-base --is-ancestor <branch> 11104c8`).
"VERIFY / gates" points to the report section holding the evidence; Z1 re-confirms the outcome
and copies it here. "Audit ref" = the report's third-party/audit table or the section that
replaces it, where one exists.

### 1.1 Phase 0 (merged before this run's packets; see `docs/zadloop/packets/FINAL_REPORT.md`)

| Packet    | State                                                   | Evidence path                                                                                             | VERIFY / gates                            | Audit ref | Fleet decisions                    |
| --------- | ------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- | ----------------------------------------- | --------- | ---------------------------------- |
| F0        | merged (`debc525`, `5b2bf5f`)                           | `docs/zadloop/R-NEXT-OWNER-CHECKLIST.md`                                                                  | FINAL_REPORT §1                           | —         | `D-RN-AUTONOMY`                    |
| F1        | merged (`a1a548f`)                                      | `docs/zadloop/packets/F1-report.md`                                                                       | FINAL_REPORT §1                           | —         | `D-RN-F1-1`                        |
| F2        | merged (`d71f66d`), one gate blocked then closed by A10 | `docs/zadloop/packets/F2-report.md`                                                                       | FINAL_REPORT §2                           | —         | `D-RN-F2-2` (cited in FIN4-report) |
| F3        | merged (`c3a2d14`); pairing S2–S5 owner-side            | `docs/zadloop/packets/F3-report.md`                                                                       | FINAL_REPORT §4                           | —         | `D-RN-F3-1`, `D-RN-F3-2`           |
| RF2 / A10 | merged (`33162dd`, `1ef5020`)                           | `docs/zadloop/packets/RF2-report.md`                                                                      | FINAL_REPORT §2                           | —         | `D-RN-RF2-1`…`D-RN-RF2-5`          |
| FIN3      | merged                                                  | `docs/zadloop/packets/FIN3-report.md`                                                                     | FINAL_REPORT §1                           | —         | `D-RN-FIN3-1`                      |
| FIN4      | merged                                                  | `docs/zadloop/packets/FIN4-report.md`                                                                     | FIN4-report §5.1                          | —         | —                                  |
| FIN5      | merged (tag `r-next/cp-phase0`, local)                  | `docs/zadloop/packets/FINAL_REPORT.md`                                                                    | FINAL_REPORT §3                           | —         | `D-RN-FIN5-1`                      |
| N0        | merged (`b4e1653`)                                      | `docs/zadloop/packets/N0-report.md`                                                                       | N0-report                                 | —         | `D-RN-N0-1`, `D-RN-N0-2`           |
| N1        | merged (`602f360`)                                      | no packet report in `docs/zadloop/packets/` — `DECISIONS.md` / `PROGRESS.md` entries only                 | TBD (Z1: cite PROGRESS entry or gap-note) | —         | TBD                                |
| A1        | merged (`797721c`)                                      | no packet report in `docs/zadloop/packets/` — `DECISIONS.md` "2026-11-16 — A1: Model Catalog Persistence" | TBD                                       | —         | see DECISIONS entry                |

### 1.2 Release-run packets (A11 order: B1 → A2 → B2 → A3 → A4 → B3 → N2 → N3 → D1 → D4 → D5 → E1 → E6 → E4 → E3 → E2 → E5 → A5 → Z1)

| Packet | State                                                                                                        | Merge commit                    | Evidence path                                                                        | VERIFY / gates                           | Audit ref                                                    | Fleet decisions                                |
| ------ | ------------------------------------------------------------------------------------------------------------ | ------------------------------- | ------------------------------------------------------------------------------------ | ---------------------------------------- | ------------------------------------------------------------ | ---------------------------------------------- |
| B1     | merged                                                                                                       | `dbc56f7`                       | `docs/zadloop/packets/B1-report.md`                                                  | §Tests, §Findings                        | —                                                            | `D-RN-B1-1`, `-2`, `-3`, `-4`                  |
| A2     | merged                                                                                                       | `9fdb797`                       | `docs/zadloop/packets/A2-report.md`                                                  | §VERIFY Evidence, §Gate Results          | §Audit Tables                                                | `D-RN-A2-1`                                    |
| B2     | merged                                                                                                       | `4d5337c`                       | `docs/zadloop/packets/B2-report.md`                                                  | §Verification, §Gate Results             | §Risk Classes (no audit table)                               | none logged in report — TBD check DECISIONS.md |
| A3     | merged                                                                                                       | `5abf6a5`                       | `docs/zadloop/packets/A3-report.md`                                                  | §VERIFY evidence, §Tests and gates       | —                                                            | `D-RN-A3-001`                                  |
| A4     | merged                                                                                                       | `ff41b0e`                       | `docs/zadloop/packets/A4-report.md`                                                  | §Verification Evidence, §Gate Table      | —                                                            | `D-RN-A4-001`                                  |
| B3     | merged                                                                                                       | `2c6f288`                       | `docs/zadloop/packets/B3-report.md` (+ `B3-rules-syntax.md`)                         | §Gates (cc5c88e)                         | —                                                            | `D-RN-B3-001`, `-002` (§Fleet decisions)       |
| N2     | merged                                                                                                       | `f99feee`                       | `docs/zadloop/packets/N2-report.md`                                                  | §Evidence, §Gate Results, §Tier Table    | —                                                            | `D-RN-N2-001`                                  |
| N3     | merged                                                                                                       | `8ad0bad`                       | `docs/zadloop/packets/N3-report.md`                                                  | §Evidence, §Gate Results                 | —                                                            | none logged in report — TBD                    |
| D1     | merged                                                                                                       | `c500b7a`                       | `docs/zadloop/packets/D1-report.md`                                                  | §Verification                            | —                                                            | `D-RN-D1-001`…`-004`                           |
| D4     | merged (+ seam `4907e61`)                                                                                    | `f215057`                       | `docs/zadloop/packets/D4-report.md`                                                  | §Gate table (@ 875fab0), §Acceptance map | —                                                            | `D-RN-D4-001`…`-007`                           |
| D5     | merged (+ seam `92e3225`)                                                                                    | `feda620`                       | `docs/zadloop/packets/D5-report.md`                                                  | §Gates (PM-verified @ 6163644)           | — (§Honest copy check A16.6)                                 | `D-RN-D5-001`…`-005`                           |
| E1     | merged                                                                                                       | `fe438bf`                       | `docs/zadloop/packets/E1-report.md`                                                  | §Gate Table                              | §Legacy UI Retirement Inventory                              | `D-RN-E1-001`…`-003`                           |
| E6     | merged                                                                                                       | `612f3a3`                       | `docs/zadloop/packets/E6-report.md` (+ `E6-audit.md`, `E6-notes.md`)                 | §Gates Status                            | `E6-audit.md`                                                | none logged in report — TBD                    |
| E4     | merged                                                                                                       | `c053213`                       | `docs/zadloop/packets/E4-report.md`                                                  | §Verification                            | —                                                            | `D-RN-E3/E4-001`, `-002`                       |
| E3     | merged                                                                                                       | `c997e34`                       | `docs/zadloop/packets/E3-report.md`                                                  | §Gates, §State Machine Fixtures          | §Skipped Audits (pixel-skin audit skipped, `D-RN-E3/E4-003`) | `D-RN-E3/E4-001`…`-003`                        |
| E2     | merged (Lane A `d4dc30d`, Lane B `90c1086`, wire fix `11104c8`)                                              | `d4dc30d`, `90c1086`, `11104c8` | `docs/zadloop/packets/E2-report-laneA.md`, `docs/zadloop/packets/E2-report-laneB.md` | Lane B §Gate matrix                      | —                                                            | `D-RN-E2B-001`…`-005`                          |
| **E5** | **PENDING** — branch `r-next/e5-e1-cleanup` @ `f7ab39f` (worktree `rn-e5-fix`); not merged at base `11104c8` | TBD                             | `docs/zadloop/packets/E5-report.md` (does not exist at base; owned by E5 sweep)      | TBD                                      | TBD                                                          | TBD                                            |
| **A5** | **PENDING** — branch `r-next/a5-health` @ `978e40e` (worktree `rn-a5`); not merged at base `11104c8`         | TBD                             | `docs/zadloop/packets/A5-report.md` (does not exist at base; owned by A5 audit)      | TBD                                      | TBD (HIGH/MEDIUM/LOW findings table)                         | `D-RN-A5-1` (on the A5 branch only)            |
| Z1     | this report                                                                                                  | TBD                             | `docs/zadloop/R-NEXT-FINAL-REPORT.md`                                                | §3 matrix                                | §4 index                                                     | TBD                                            |

Blocked packets in the release run: **none known at base** — Z1 re-states this after E5/A5.
Deferred by A11 (not blocked, self-hosted queue §13): C1 → C2 → C3 (R2); D2 → D3 (R3).

Notes for the fill-in:

- Merge commits above were read from `git log --merges` on `11104c8`; Z1 re-derives them after E5/A5 land.
- `PROGRESS.md` carries one dated entry per packet with cumulative spend (A15); Z1 copies the final cumulative figure (last seen in the working tree: ≈ 180 of 400 after E3/E4 — **not final**).
- Entries marked "none logged in report" need a one-grep check of `DECISIONS.md` (`D-RN-<id>`) before Z1 claims "no decisions".

---

## 2. Audit-tables index

Z1 fills this index; no content is invented here.

| Source                                                              | What it contains                                                                                                       | Status                            |
| ------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- | --------------------------------- |
| `docs/zadloop/packets/A2-report.md` §Audit Tables                   | third-party audit for `oauth2`, `url` (`THIRD_PARTY_NOTICES.md` updated)                                               | merged                            |
| `docs/zadloop/packets/E1-report.md` §Legacy UI Retirement Inventory | per-screen reachability / disposition                                                                                  | merged                            |
| `docs/zadloop/packets/E6-audit.md`                                  | E6 audit                                                                                                               | merged                            |
| `docs/zadloop/packets/E3-report.md` §Skipped Audits                 | pixel-skin audit skipped (`D-RN-E3/E4-003`)                                                                            | merged                            |
| `docs/zadloop/packets/A5-report.md`                                 | code-health audit, HIGH/MEDIUM/LOW table, dead code / `TODO`/`FIXME` sweep, largest files, duplication, docs freshness | **PENDING (A5)**                  |
| `docs/zadloop/packets/E5-report.md`                                 | visual-baseline sweep, e2e debt closure                                                                                | **PENDING (E5)**                  |
| `THIRD_PARTY_NOTICES.md`, `deny.toml`                               | A3 §3.3 audit-before-adoption state                                                                                    | TBD (A5 refreshes; re-read at Z1) |

---

## 3. FIN4 matrix on `r-next/integration` (Z1 run)

Sources of the rows: master plan §3.1.9 ("Suites green"), §4 FIN4, `docs/zadloop/packets/R-FIN-audit.md`
"FIN4 verification matrix (§5)", `docs/zadloop/packets/FIN4-report.md` §5.1, and `docs/VERIFY.md`.
Commands are the ones `FIN4-report.md` §5.1 / `package.json` record; Z1 re-checks them against
`docs/VERIFY.md` before running. "Prior recorded" is context from `PROGRESS.md` (integration @
`8fd900c`, pre-E2/E5/A5) and is **not** a Z1 result.

Z1 run fields: tree = `r-next/integration` @ `TBD` (after E5 + A5 merge); date `TBD`; gate-log directory `TBD`.

| #   | Row                                                                      | Command                                                                                       | Z1 result                                                                                         | Evidence (gate log)   | Prior recorded (reference only)                |
| --- | ------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- | --------------------- | ---------------------------------------------- |
| 1   | Rust format                                                              | `cargo fmt --check`                                                                           | TBD                                                                                               | TBD                   | clean                                          |
| 2   | Rust lint                                                                | `cargo clippy --workspace --all-targets -- -D warnings`                                       | TBD                                                                                               | TBD                   | 0                                              |
| 3   | Rust tests                                                               | `cargo test --workspace`                                                                      | TBD                                                                                               | TBD                   | all ok / 0 FAILED                              |
| 4   | TS typecheck                                                             | `pnpm typecheck`                                                                              | TBD                                                                                               | TBD                   | tsc desktop 0 errors                           |
| 5   | TS lint                                                                  | `pnpm lint` (official scope `eslint src`)                                                     | TBD                                                                                               | TBD                   | 1 pre-existing warning (FIN4)                  |
| 6   | Unit suites                                                              | `pnpm test` (client, i18n, ui, crypto, remote, desktop)                                       | TBD                                                                                               | TBD                   | all pass                                       |
| 7   | i18n gate                                                                | `node scripts/ci/i18n-gate.mjs`                                                               | TBD                                                                                               | TBD                   | 0 violations                                   |
| 8   | v3 non-visual e2e                                                        | desktop + remote Playwright, non-`@visual`                                                    | TBD                                                                                               | TBD                   | 14 pre-existing failures recorded for E5       |
| 9   | Wire contracts                                                           | `node scripts/check-wire-contract.mjs`                                                        | TBD                                                                                               | TBD                   | 4 violations (A1 `catalog.refresh/apply` debt) |
| 10  | Arch rules                                                               | `node scripts/check-arch.mjs` (`pnpm arch`)                                                   | TBD                                                                                               | TBD                   | 5 rules / 0                                    |
| 11  | Policy                                                                   | `node scripts/check-engineering-policy.mjs` (`pnpm policy`)                                   | **0 failures at base `11104c8`** (8 checks, exit 0, 2026-11-19, this worktree) — re-run at Z1 tip | this session (see §8) | 8 / 0                                          |
| 12  | Versions                                                                 | `node scripts/check-versions.mjs` (`pnpm versions`)                                           | TBD                                                                                               | TBD                   | aligned, 0.3.0 expected                        |
| 13  | Actions gate                                                             | `node scripts/ci/actions-gate.mjs`                                                            | TBD                                                                                               | TBD                   | 19 pre-existing violations                     |
| 14  | ds-v3                                                                    | `node scripts/check-ds-v3.mjs apps/desktop/src/v3 packages/i18n` (canonical form)             | TBD                                                                                               | TBD                   | 43 hits = pre-existing baseline                |
| 15  | cargo deny                                                               | `cargo deny --config deny.toml check` (+ `apps/desktop/src-tauri` manifest)                   | TBD                                                                                               | TBD                   | advisories failing (see §4 R1)                 |
| 16  | gitleaks                                                                 | `gitleaks detect` (git mode; `--no-git` noted separately)                                     | TBD                                                                                               | TBD                   | git-mode clean                                 |
| 17  | Gate-tool self-tests                                                     | `pw-gate.mjs --self-test`, `pw-genesis.mjs --self-test`, `actions-gate.mjs --self-test`       | TBD                                                                                               | TBD                   | pass                                           |
| 18  | @visual verify (desktop 416 + remote 84, A10 budget `maxDiffPixels: 64`) | CI `capture-baselines.yml` verify legs                                                        | TBD (needs CI; baselines changed by E5?)                                                          | TBD (CI run ids)      | 3 green CI runs at Phase 0 close               |
| 19  | Workflows present                                                        | `.github/workflows/` (`ci.yml`, `capture-baselines.yml`, `desktop-bundle.yml`, `release.yml`) | TBD                                                                                               | TBD                   | present (FIN4)                                 |
| 20  | Mock-provider greenfield e2e                                             | `cargo test -p warsha-pipeline --test fake_e2e`                                               | TBD                                                                                               | TBD                   | 6/6 at Phase 0                                 |
| 21  | `--self-check` (release build)                                           | `warsha-desktop --self-check`                                                                 | TBD (host permitting)                                                                             | TBD                   | 12/12 ×3 at Phase 0                            |
| 22  | Docs consistency audit                                                   | `docs/VERIFY.md` §1.1 commands (run from repo root)                                           | TBD                                                                                               | TBD                   | —                                              |

Owner-only / uncovered rows (recorded, not performed; see §4):

| Row                                                  | Why uncovered                                             | Owner-checklist item |
| ---------------------------------------------------- | --------------------------------------------------------- | -------------------- |
| T2 Windows leg                                       | no Windows runner on the run host                         | item 4               |
| Installers (desktop bundles, APK release builds)     | A9: CI-built from a tagged tree when owner orders release | item 2               |
| T3 pairing S2–S5 (R-UI-7b physical device)           | needs two devices / owner hardware                        | item 3               |
| Install on owner PC, `--self-check` on installed app | owner-only                                                | item 2               |
| `main` fast-forward                                  | owner-only (§12)                                          | item 1               |

---

## 4. Open risks

Each risk keeps its evidence pointer; Z1 updates state after E5/A5 and removes any that closed.

| ID  | Risk                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 | State at base `11104c8`                        | Evidence                                                                                                                |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| R1  | **A5 advisory stack (`D-RN-A5-1`)** — `cargo deny` advisories: `RUSTSEC-2025-0134` (`rustls-pemfile 1.0.4` via `reqwest 0.11.27 <- oauth2 4.4.2 <- warsha-providers`) plus further 2026 advisory-DB notices on `chrono`, `h2`, `rustls 0.21.12`, `rustls-webpki 0.101.7`; the A5 branch counts 10 in the root workspace and 11 in `apps/desktop/src-tauri` (adds `zbus_names`). Deliberately **not** ignored by A5. Owner decision: upgrade the TLS stack / `oauth2` 5.x, or accept by `OD-S*` ID with matching `deny.toml` ignores. | OPEN pending A5 merge; branch-only today       | `DECISIONS.md` `D-RN-A5-1` and `docs/zadloop/R-NEXT-OWNER-CHECKLIST.md` item 14, both on `r-next/a5-health` @ `978e40e` |
| R2  | **RF2 / A10 tolerance** — `@visual` compare carries a bounded 64-px decoded-pixel budget (`maxDiffPixels: 64`, threshold `0.05` unchanged, `maxDiffPixelRatio` unset). Scoped to `@visual` only; later UI packets (E1–E5) must re-capture on CI, and any drift above budget is a real regression.                                                                                                                                                                                                                                    | accepted by owner (A10); re-verify at Z1       | `docs/zadloop/packets/RF2-report.md`; `DECISIONS.md` `D-RN-RF2-5`; checklist item 13 (closed)                           |
| R3  | **F1 / F3-blocked rows** — F3 pairing S2–S5 (`@JavascriptInterface`/Keystore boundary) needs physical hardware; smoke-script S4/S5 harness follow-up is an owner decision. F1 capture host is CI-only (no local pinned host) so any baseline marked `pending` stays an owner item.                                                                                                                                                                                                                                                   | OPEN (owner-side)                              | `docs/zadloop/packets/F3-report.md`; checklist items 3, 5, 12                                                           |
| R4  | **T2 Windows / installers / owner PC** — no Windows runner; installers not built (A9); nothing installed on the owner's PC; `main` not advanced.                                                                                                                                                                                                                                                                                                                                                                                     | OPEN (owner-side)                              | `docs/zadloop/packets/FINAL_REPORT.md` §4; checklist items 1, 2, 4                                                      |
| R5  | **Pre-existing non-visual debt carried to Z1** — wire contract A1 debt (`catalog.refresh`/`catalog.apply`), actions-gate 19 violations, ds-v3 baseline hits (`packages/ui/src/v3/composites/Gauge.css` among them), 14 pre-existing desktop e2e failures from the E1 three-rung mode contract. E5 is expected to address the e2e set; the rest are recorded, not fixed, unless E5/A5 say otherwise.                                                                                                                                  | TBD after E5/A5                                | `PROGRESS.md` "E4 + E3 completed and merged" entry; E5/A5 reports (pending)                                             |
| R6  | **Live-service checks never run** — OAuth live sign-in per shipped lane (A2), provider balance APIs (A3), phone approval over Tailscale (E2), codex CLI probe (N0). Tests use the mock provider / local stubs only (§3.1.11).                                                                                                                                                                                                                                                                                                        | OPEN (owner-side)                              | checklist items 6, 7, 8, "Fleet notes"                                                                                  |
| R7  | **PM-takeover lanes** — D1, D4, D5, E3, E4 finished by PM after worker correction budgets were exhausted; lane-health notes record false gate claims (`D-RN-D4-006`). Gate evidence for those packets is PM-verified; Z1 should prefer re-measured numbers.                                                                                                                                                                                                                                                                          | recorded                                       | `DECISIONS.md` `D-RN-D4-006`, `D-RN-D4-007`; `PROGRESS.md` D4/D5/E3/E4 entries                                          |
| R8  | **A5 structural findings** — by A5 rule, anything structural is not fixed there; it is recorded here and in the checklist with a recommendation.                                                                                                                                                                                                                                                                                                                                                                                     | TBD (A5 report pending)                        | `docs/zadloop/packets/A5-report.md` (pending)                                                                           |
| R9  | **Spend** — cumulative estimated spend vs the 400 ceiling (A1/A15); alert threshold 320.                                                                                                                                                                                                                                                                                                                                                                                                                                             | TBD (final figure from `PROGRESS.md` after A5) | `PROGRESS.md` last packet entry; checklist item 11                                                                      |

---

## 5. Recommended next version

**Recommendation only — this report and this branch do not bump anything. All versions stay `0.3.0`.**

| Field            | Value                                                                                                                                                                     |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Current          | `0.3.0` (mobile versionCode 300 per FIN4)                                                                                                                                 |
| Recommended next | `TBD` — Z1 proposes one value with the reasoning (scope shipped: permissions, providers/OAuth, failover, modes, swarm, secretary, viewer; release R1 per master plan §13) |
| Rationale inputs | merged-packet list (§1), open risks (§4), any breaking migration notes (`D-RN-B1-2` legacy permission bridge; `D-RN-D4-004` migration 016; E2 migration 018)              |
| Owner action     | checklist item 10 — owner decides, bumps, and releases                                                                                                                    |

---

## 6. Owner checklist pointer

Owner checklist: [`docs/zadloop/R-NEXT-OWNER-CHECKLIST.md`](R-NEXT-OWNER-CHECKLIST.md).

- At base `11104c8` it holds items 1–13 (item 13 closed by A10). Item 14 (cargo-deny advisory gate, from `D-RN-A5-1`) exists only on `r-next/a5-health`.
- **Not finalised here.** Z1 finalises it after E5/A5 merge (§10 scope: "Finalise the owner checklist"). A5 owns edits to that file until then; this skeleton does not touch it.
- Z1 finalisation to-do: confirm each item's packet reference, move closed items, add E5/A5 additions, reconcile with plan §10.2 known items.

---

## 7. Z1 run to-do (not performed by this skeleton)

1. After E5 and A5 merge into `r-next/integration`, re-base this draft on the new tip; update the header base commit.
2. Run the full FIN4 matrix (§3) on `r-next/integration`; store gate logs; fill the Z1 result and evidence columns.
3. Fill §1 pending rows (E5, A5) from their reports; resolve every "TBD" / "none logged" cell with an evidence path.
4. Fill §2, §4, §5; finalise the owner checklist (§6).
5. Tag `r-next/cp-final` locally; push the integration branch non-force to `warsha-dev` (master plan §10). **Not done by this scaffold. Never `main`, never release, never force.**
6. Verify `main` is unchanged from the start of the run; remove the DRAFT banner only after all of the above.

---

## 8. Scaffold provenance

- Created on branch `rn-z1` at base `11104c8`; docs-only; no TS/Rust touched; no strings added.
- Read-only checks at scaffold time (worktree `.agents/worktrees/rn-z1`): `node scripts/check-engineering-policy.mjs` → `policy: 8 checks, 0 failures`, exit 0. No heavy suites run (Tester lane owns the baseline).
- Sources read: `docs/zadloop/R-NEXT-MASTER-PLAN.md` §3.1, §3.4, §4 (FIN4/FIN5), §10, §11, A5/A10/A11; `docs/zadloop/packets/*-report.md`; `docs/zadloop/packets/FINAL_REPORT.md`; `docs/zadloop/packets/FIN4-report.md`; `docs/zadloop/packets/R-FIN-audit.md`; `DECISIONS.md`; `PROGRESS.md`; `git show r-next/a5-health:DECISIONS.md` (read-only).
