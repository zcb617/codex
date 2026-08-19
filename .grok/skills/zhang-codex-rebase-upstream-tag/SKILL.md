---
name: zhang-codex-rebase-upstream-tag
description: >
  Rebase this Codex fork's custom commits onto a new upstream rust-vX.Y.Z release tag:
  fetch the tag, create dev/from-rust-vX.Y.Z, port PostCompact, skills metadata budget,
  and multi-platform CI changes, update the GitHub Actions branch trigger, verify compile,
  and commit. Use when the user says upstream has a new tag (e.g. rust-v0.146.0), wants
  to "基于上游新TAG建分支", "合并本地修改", "像之前升0.145一样", or runs
  /zhang-codex-rebase-upstream-tag.
---

# Rebase custom fork work onto a new upstream rust tag

Automate the recurring release-branch upgrade for **this** Codex fork (`zcb617/codex` / `my_codex`):

1. Base on upstream `rust-vX.Y.Z`
2. Create `dev/from-rust-vX.Y.Z`
3. Port local custom commits (PostCompact + skills metadata budget + CI)
4. Point GitHub CI at the new branch name
5. Compile-check and commit (do **not** push unless the user explicitly asks)

Always answer the user in **中文**.

## Prerequisites / remotes

- Working tree should be clean before starting (stash or commit unrelated work).
- Remotes:
  - `origin` → user fork (e.g. `zcb617/codex`)
  - `upstream` → `https://github.com/openai/codex.git` (add if missing)
- Source branch for custom work is usually the previous release branch, e.g. `dev/from-rust-v0.145.0` (or whatever is currently checked out / latest `dev/from-rust-v*`).

## Inputs

Ask only if not already stated:

- **New tag**: e.g. `rust-v0.146.0` (must exist on upstream)
- **Source branch** (optional): previous customized branch; default = current `dev/from-rust-v*` that tracks the last done upgrade

Do **not** push without explicit user authorization.

## Custom content to preserve

These are the fork-local changes that must land on every new tag branch (names/shas vary; identify by **content**, not only commit message):

### A. Product feature — PostCompact `additionalContext`

- Hooks parse PostCompact `hookSpecificOutput.additionalContext`
- After successful compaction, inject context **immediately** (not only via SessionStart compact on next user turn)
- Typical paths:
  - `codex-rs/hooks/src/events/compact.rs`
  - `codex-rs/hooks/src/engine/mod.rs` / `output_parser.rs` / `schema.rs`
  - `codex-rs/hooks/schema/generated/post-compact.command.output.schema.json`
  - `codex-rs/core/src/hook_runtime.rs` (`record_additional_contexts` after post-compact)
  - `codex-rs/core/tests/suite/hooks.rs` (helpers + tests)
  - `codex-rs/hooks/POST_COMPACT_ADDITIONAL_CONTEXT.md`

**API drift note:** Upstream hooks may change internal types (e.g. `Vec<String>` → `AdditionalContext` + `output_spiller`). Port semantics; adapt to current upstream helpers (mirror SessionStart / UserPromptSubmit). Wire JSON for plugins stays camelCase string `additionalContext` unless upstream deliberately changes the public schema.

### B. Multi-platform CLI GitHub Actions workflow

- File: `.github/workflows/build-windows-codex.yml` (filename historical; covers Win + Linux + macOS)
- On `push` / `workflow_dispatch` for branch `dev/from-rust-vX.Y.Z`
- Jobs (parallel):
  - **Windows** `x86_64-pc-windows-msvc`: `codex`, `codex-code-mode-host`, `codex-windows-sandbox-setup`, `codex-command-runner` → sibling `dist/*.exe`
  - **Linux** `ubuntu-latest` host build: `codex`, `codex-code-mode-host`
  - **macOS** matrix `macos-latest`: `aarch64-apple-darwin` + `x86_64-apple-darwin` → `codex` + `codex-code-mode-host`
- Artifacts: platform-named, retention ~14 days

When upgrading, **always set** the workflow `push.branches` entry to the **new** branch name.

### C. Skills metadata context budget (2% / 8_000 → 4% / 16_000)

Increase the share of model-visible context used for the skills catalog.

Target semantics (identify by **content**, never by SHA such as `102356cde1`):

- Default character budget: **16_000** (upstream is typically `8_000`)
- Context-window share: **4%** (upstream is typically `2%`)
- If upstream already uses a **larger** char budget or percent, keep upstream — do not lower it

Search for these names (they have moved across crates):

- `DEFAULT_SKILL_METADATA_CHAR_BUDGET`
- `SKILL_METADATA_CONTEXT_WINDOW_PERCENT`
- Host caps if present: `MAX_HOST_SKILLS_METADATA_CHARS` / `MAX_HOST_SKILLS_METADATA_TOKENS` (do not leave a host clamp that undoes the 16_000 / 4% catalog budget)
- User-facing copy that hard-codes `"2% skills context budget"` — update the percent to match

Known locations (use whichever exist on the new tag):

- `codex-rs/ext/skills/src/render.rs` (0.147.x)
- `codex-rs/core-skills/src/render.rs` (later main)
- Matching tests next to those files (`render_tests.rs`, `skills_extension.rs`, in-file `#[cfg(test)]`)

**API drift note:** Do not cherry-pick the old SHA if paths moved. Patch the current constants and any assertions that encode `2_000` tokens @ 100k window, `8_000` tokens @ 400k window, or `Characters(8_000)` with no context window.

## Procedure

### 1. Fetch the tag

```bash
git remote add upstream https://github.com/openai/codex.git 2>/dev/null || true
git fetch upstream tag rust-vX.Y.Z --no-tags
git rev-parse rust-vX.Y.Z^{commit}
```

Confirm the tag exists. If missing, stop and tell the user.

### 2. Identify custom commits on the source branch

```bash
# Example: previous branch was based on rust-v0.145.0
git log --oneline rust-v0.145.0..dev/from-rust-v0.145.0
git diff --name-only rust-v0.145.0..dev/from-rust-v0.145.0
```

Prefer a **small clean set** on the new branch rather than replaying every intermediate CI branch-rename commit:

| Keep | Drop / fold |
|------|-------------|
| PostCompact feature commit(s) | Intermediate “point CI at 0.144.x / 0.145.x” renames |
| Hooks API adapt commit (if separate) | Redundant lockfile thrash from old minor versions |
| Docs for PostCompact | — |
| Skills metadata budget (content-port if paths drifted) | Blind cherry-pick of old `Increase skills metadata context budget` SHA when files moved |
| **One** final multi-platform workflow commit for the new branch | Stacked tiny workflow edits |

Optional: save the current workflow file before switching:

```bash
cp .github/workflows/build-windows-codex.yml /tmp/build-cli-workflow-prev.yml
```

### 3. Create the new branch from the tag

```bash
git checkout -b dev/from-rust-vX.Y.Z rust-vX.Y.Z
```

### 4. Port custom commits

**Preferred:** cherry-pick feature/adapt/docs commits in order, resolve conflicts carefully. Port the skills budget by content (section C) if the old commit does not apply cleanly.

```bash
git cherry-pick <postcompact-sha> <api-adapt-sha> <docs-sha>
# Then port section C (skills budget) if cherry-pick of that change fails or files moved
```

On conflicts in `hooks.rs` / `compact.rs`:

- Keep **both** upstream helpers/tests and PostCompact helpers/tests when they coexist
- Re-adapt PostCompact to current upstream `AdditionalContext` / `append_additional_context` / `output_spiller` patterns
- Do not drop `record_additional_contexts` after post-compact in `hook_runtime.rs`

**Workflow:** do not replay every CI micro-commit. Instead:

```bash
mkdir -p .github/workflows
# Start from previous final workflow, rewrite branch trigger
sed 's/dev\/from-rust-vOLD/dev\/from-rust-vX.Y.Z/g' /tmp/build-cli-workflow-prev.yml \
  > .github/workflows/build-windows-codex.yml
```

Or rewrite the branch name in place if the file already cherry-picked.

### 5. Cargo.lock hygiene

- Prefer the **tag’s** `codex-rs/Cargo.lock` unless source changes require real dependency updates.
- If an old adapt commit dragged in a previous minor lockfile, restore:

```bash
git checkout rust-vX.Y.Z -- codex-rs/Cargo.lock
```

### 6. Verify

Minimum:

```bash
cd codex-rs
cargo check -p codex-hooks
cargo check -p codex-core
```

After porting, confirm skills budget landed:

```bash
rg -n 'DEFAULT_SKILL_METADATA_CHAR_BUDGET|SKILL_METADATA_CONTEXT_WINDOW_PERCENT|2% skills context budget' \
  --glob '*.rs' codex-rs
```

Expect `16_000` and `4` (or larger if upstream already exceeded those). Fail the upgrade if the new branch still has `8_000` / `2` as the catalog defaults.

Optional (longer, if user asked to package/build):

```bash
cargo build --release -p codex-cli
./target/release/codex --version   # expect matching X.Y.Z
```

### 7. Commit

Commit after the task (user rule). Suggested commits:

1. Feature + adapt + docs (if not already from cherry-pick)
2. Skills metadata budget (if not already from cherry-pick)
3. Workflow for the new branch (if separate)
4. Lockfile restore (only if needed)

Example messages:

```text
Allow PostCompact hooks to inject additionalContext after compaction.
Adapt PostCompact additionalContext to current hooks APIs.
Document PostCompact additionalContext injection for developers.
Increase skills metadata context budget
Add multi-platform CLI CI for Windows, Linux, and macOS.
Restore Cargo.lock to rust-vX.Y.Z baseline.
```

### 8. Report and stop for push

Tell the user in 中文:

- New branch name and base tag
- Commits on top of the tag (`git log --oneline rust-vX.Y.Z..HEAD`)
- `git describe --tags --always HEAD`
- Compile check result
- That the branch is **local only** until they authorize `git push -u origin dev/from-rust-vX.Y.Z`

**Never `git push` without explicit user approval.**

## CI workflow checklist (must match)

After upgrade, `.github/workflows/build-windows-codex.yml` must:

- [ ] `on.push.branches` includes **exactly** `dev/from-rust-vX.Y.Z`
- [ ] Windows builds four bins (CLI + code-mode-host + two sandbox helpers)
- [ ] Linux builds `codex` + `codex-code-mode-host`
- [ ] macOS matrix builds arm64 + x86_64 (`codex` + `codex-code-mode-host`)
- [ ] Artifacts uploaded per platform

## Conflict / failure playbook

| Symptom | Action |
|---------|--------|
| Tag missing | `git ls-remote --tags upstream 'rust-v*'` and confirm with user |
| Cherry-pick conflict in tests | Keep upstream tests + PostCompact tests side by side |
| Type errors on `additional_contexts` | Follow current `session_start.rs` / `user_prompt_submit.rs` patterns |
| Workflow missing on tag branch | Re-add full multi-platform workflow from previous branch |
| Skills budget cherry-pick fails / files missing | Search the names in section C; patch current crate (`ext/skills` or `core-skills`); update tests and `2%` copy |
| New branch still shows 8_000 / 2% | Stop; budget port did not land |
| Dirty `Cargo.lock` after check | Prefer tag lockfile if no new crates |

## Out of scope

- Syncing GitHub “Sync fork” alone does **not** replace this flow (tags/branches are not auto-mirrored forever).
- Desktop App “Computer Use” packaging (x64 missing plugin) is **not** fixed by this skill.
- Do not force-push or rewrite published history without user request.

## Quick command summary

```bash
TAG=rust-v0.146.0
PREV=dev/from-rust-v0.145.0   # adjust
BRANCH=dev/from-rust-v0.146.0

git fetch upstream tag "$TAG" --no-tags
git checkout -b "$BRANCH" "$TAG"
# cherry-pick PostCompact feature/adapt/docs...
# port skills metadata budget (16_000 / 4%) by content if needed
# install workflow with branch = $BRANCH
cd codex-rs && cargo check -p codex-hooks && cargo check -p codex-core
# commit remaining changes
# wait for user before: git push -u origin "$BRANCH"
```
