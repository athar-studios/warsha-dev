# Baseline P0 Evidence - Warsha 0.3.0 Release Gate

## Environment
- Worktree: .agents/worktrees/r-next-integration
- Revision: 3c8b3a1

## Gates

### 1. cargo fmt --check (root)
Command: `CARGO_HOME=/tmp/cargo-home cargo fmt --check`
Result: Exit Code 0 (Passed)

### 2. cargo fmt --check (apps/desktop/src-tauri)
Command: `CARGO_HOME=/tmp/cargo-home cargo -C apps/desktop/src-tauri fmt --check`
Result: Exit Code 0 (Passed)

### 3. node scripts/check-arch.mjs
Command: `node scripts/check-arch.mjs`
Result: Exit Code 0 (Passed, 5 rules, 0 violations)

### 4. node scripts/check-wire-contract.mjs
Command: `node scripts/check-wire-contract.mjs`
Result: Exit Code 1 (Failed, 11 checks, 4 violation(s))

### 5. node scripts/check-engineering-policy.mjs
Command: `node scripts/check-engineering-policy.mjs`
Result: Exit Code 0 (Passed, 8 checks, 0 failures)

### 6. node scripts/check-versions.mjs
Command: `node scripts/check-versions.mjs`
Result: Exit Code 0 (Passed, versions aligned)

### 7. npx eslint --no-cache apps/desktop/src
Command: `npx eslint --no-cache apps/desktop/src`
Result: Exit Code 1 (Failed, 18 problems: 17 errors, 1 warning)

### 8. cargo deny check (root)
Command: `CARGO_HOME=/tmp/cargo-home cargo deny check`
Result: Exit Code 1 (Failed, advisories FAILED)

## Grep Records
- `grep -rn sayl.site --include='*.rs' --include='*.sh' --include='*.example' crates docker deploy | head`
  Found 6 occurrences in `crates/warsha-providers/src/env_llm.rs`, `docker/entrypoint.sh`, and `deploy/.env.example`.
- `grep -rn PLACEHOLDER_SHA256 .github/workflows/release.yml | head`
  Found 3 occurrences.
- `git ls-files bridge-io | wc -l`
  71
