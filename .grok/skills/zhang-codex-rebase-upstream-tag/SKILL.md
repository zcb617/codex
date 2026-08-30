---
name: zhang-codex-rebase-upstream-tag
description: >
  Rebase this Codex fork's custom commits onto a new upstream rust-vX.Y.Z
  release tag, keep the fork CLI publisher, delete upstream release workflows
  that the tag brings back, then (only with user approval) push the branch and
  retarget rust-vX.Y.Z onto the fork HEAD so Actions builds and attaches
  binaries to a GitHub Release. Use when the user says upstream has a new tag
  (e.g. rust-v0.146.0), wants to "基于上游新TAG建分支", "合并本地修改",
  "像之前升0.145一样", "发版", "打tag发release", or runs
  /zhang-codex-rebase-upstream-tag.
---

# Rebase custom fork work onto a new upstream rust tag

Automate the recurring upgrade and CLI publish for **this** Codex fork (`zcb617/codex` / `my_codex`):

1. Base on upstream `rust-vX.Y.Z`
2. Create `dev/from-rust-vX.Y.Z`
3. Port local custom commits (PostCompact + skills metadata budget + fork CLI publisher)
4. Delete the upstream release workflows that the new tag brings back
5. Compile-check and commit
6. Stop for push. After the user authorizes: push the branch, retarget `rust-vX.Y.Z` onto **fork HEAD**, push that tag so Actions publishes the GitHub Release

Always answer the user in **中文**.

## Prerequisites / remotes

- Working tree should be clean before starting (stash or commit unrelated work).
- Remotes:
  - `origin` → user fork (e.g. `zcb617/codex`)
  - `upstream` → `https://github.com/openai/codex.git` (add if missing)
- Source branch for custom work is usually the previous release branch, e.g. `dev/from-rust-v0.149.1` (or whatever is currently checked out / latest `dev/from-rust-v*`).

## Inputs

Ask only if not already stated:

- **New tag**: e.g. `rust-v0.150.0` (must exist on upstream)
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

### B. Fork CLI publisher

- File: `.github/workflows/build-windows-codex.yml` (filename historical; covers Win + Linux + macOS)
- `on.push.tags: rust-v*.*.*` plus `workflow_dispatch`
- Jobs (parallel):
  - **Windows** `x86_64-pc-windows-msvc`: `codex`, `codex-code-mode-host`, `codex-windows-sandbox-setup`, `codex-command-runner` → sibling `dist/*.exe`
  - **Linux** `ubuntu-latest` host build: `codex`, `codex-code-mode-host`
  - **macOS** matrix `macos-latest`: `aarch64-apple-darwin` + `x86_64-apple-darwin` → `codex` + `codex-code-mode-host`
- Tag builds: `publish-github-release` waits for all platform jobs, packs archives, creates/updates the GitHub Release for that tag
- The publish job does **not** checkout the repo; it must set `GH_REPO: ${{ github.repository }}` so `gh release` can see the fork
- `workflow_dispatch` uploads platform artifacts (retention ~14 days) but does not create a Release

Copy this file from the previous fork branch. Do not convert it back to a branch trigger.

### C. Upstream release workflows that must stay deleted

Checking out an upstream `rust-v*` tag **restores** openai/codex publishers. This fork cannot run them (self-hosted runners, signing, R2, PyPI). After creating the new branch, those files must be gone again:

- `.github/workflows/rust-release.yml`
- `.github/workflows/rust-release-windows.yml`
- `.github/workflows/rust-release-zsh.yml`
- `.github/workflows/rust-release-prepare.yml`
- `.github/workflows/rust-release-argument-comment-lint.yml`
- `.github/workflows/r2-release.yml`
- `.github/workflows/python-sdk-release.yml`
- `.github/workflows/python-runtime-release.yml`
- `.github/workflows/python-runtime-build.yml`
- `.github/workflows/rusty-v8-release.yml`

Prefer cherry-picking the existing “Drop upstream release workflows…” commit from the previous fork branch. If that commit does not apply, `git rm` the list above and keep section B.

Drop the CODEOWNERS line for `rust-release.yml` and the `rusty-v8-release.yml` paths in `.github/scripts/v8_canary_changes.py` if they come back.

### D. Skills metadata context budget (2% / 8_000 → 4% / 16_000)

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
# Example: previous branch was based on rust-v0.149.1
git log --oneline rust-v0.149.1..dev/from-rust-v0.149.1
git diff --name-only rust-v0.149.1..dev/from-rust-v0.149.1
```

Prefer a **small clean set** on the new branch rather than replaying every intermediate CI rename:

| Keep | Drop / fold |
|------|-------------|
| PostCompact feature commit(s) | Intermediate “point CI at 0.144.x / 0.145.x” branch-trigger renames |
| Hooks API adapt commit (if separate) | `v*.*.*` / `dev/from-rust-v*` tag-trigger experiments |
| Docs for PostCompact | Redundant lockfile thrash from old minor versions |
| Skills metadata budget (content-port if paths drifted) | Blind cherry-pick of old `Increase skills metadata context budget` SHA when files moved |
| Final fork CLI publisher (`build-windows-codex.yml` on `rust-v*.*.*` **with `GH_REPO`**) | Stacked tiny workflow edits; publisher copies that omit `GH_REPO` |
| Drop-upstream-release-workflows commit | — |

Optional: save the current publisher before switching:

```bash
cp .github/workflows/build-windows-codex.yml /tmp/build-cli-workflow-prev.yml
```

### 3. Create the new branch from the tag

```bash
git checkout -b dev/from-rust-vX.Y.Z rust-vX.Y.Z
```

This checkout restores section C files. That is expected; remove them in step 4.

### 4. Port custom commits

**Preferred:** cherry-pick feature/adapt/docs/budget/publisher/delete-upstream-release commits in order, resolve conflicts carefully. Port the skills budget by content (section D) if the old commit does not apply cleanly.

```bash
git cherry-pick <postcompact-sha> <api-adapt-sha> <docs-sha> <drop-upstream-release-sha>
# Then port section D (skills budget) if cherry-pick of that change fails or files moved
```

On conflicts in `hooks.rs` / `compact.rs`:

- Keep **both** upstream helpers/tests and PostCompact helpers/tests when they coexist
- Re-adapt PostCompact to current upstream `AdditionalContext` / `append_additional_context` / `output_spiller` patterns
- Do not drop `record_additional_contexts` after post-compact in `hook_runtime.rs`

**Workflow:** do not replay every CI micro-commit. Instead copy the previous final publisher and confirm section C is gone:

```bash
mkdir -p .github/workflows
cp /tmp/build-cli-workflow-prev.yml .github/workflows/build-windows-codex.yml
git rm -f --ignore-unmatch \
  .github/workflows/rust-release.yml \
  .github/workflows/rust-release-windows.yml \
  .github/workflows/rust-release-zsh.yml \
  .github/workflows/rust-release-prepare.yml \
  .github/workflows/rust-release-argument-comment-lint.yml \
  .github/workflows/r2-release.yml \
  .github/workflows/python-sdk-release.yml \
  .github/workflows/python-runtime-release.yml \
  .github/workflows/python-runtime-build.yml \
  .github/workflows/rusty-v8-release.yml
```

`on.push.tags` must be `rust-v*.*.*`. The `publish-github-release` job must still exist and set `GH_REPO` (it does not checkout).

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

Confirm publishers:

```bash
test -f .github/workflows/build-windows-codex.yml
test ! -e .github/workflows/rust-release.yml
rg -n 'rust-v\*\.\*\.\*|publish-github-release|GH_REPO' .github/workflows/build-windows-codex.yml
```

Fail the upgrade if `GH_REPO` is missing from the publish job. Without it, packing succeeds then `gh release` dies with `not a git repository`.

Optional (longer, if user asked to package/build):

```bash
cargo build --release -p codex-cli
./target/release/codex --version   # expect matching X.Y.Z
```

### 7. Commit

Commit after the task (user rule). Suggested commits:

1. Feature + adapt + docs (if not already from cherry-pick)
2. Skills metadata budget (if not already from cherry-pick)
3. Fork CLI publisher (if separate)
4. Drop upstream release workflows (if not already from cherry-pick)
5. Lockfile restore (only if needed)

Example messages:

```text
Allow PostCompact hooks to inject additionalContext after compaction.
Adapt PostCompact additionalContext to current hooks APIs.
Document PostCompact additionalContext injection for developers.
Increase skills metadata context budget
Add multi-platform CLI CI for Windows, Linux, and macOS.
Drop upstream release workflows and publish from rust-v* tags.
Restore Cargo.lock to rust-vX.Y.Z baseline.
```

### 8. Report and stop for push

Tell the user in 中文:

- New branch name and base tag
- Commits on top of the tag (`git log --oneline rust-vX.Y.Z..HEAD`)
- `git describe --tags --always HEAD` (this will usually show the **upstream** tag, not fork HEAD)
- Compile check result
- That the branch is **local only** until they authorize push
- The publish commands below, and that they are **not** run until authorized

**Never `git push` without explicit user approval.**

### 9. Publish CLI GitHub Release (only after the user says to push)

The fetched `rust-vX.Y.Z` points at the **upstream** commit. Actions must build **fork HEAD** (PostCompact + publisher workflow). Retarget the tag onto HEAD before pushing it to `origin`.

```bash
BRANCH=dev/from-rust-vX.Y.Z
TAG=rust-vX.Y.Z

git push -u origin "$BRANCH"

# Move the local tag from upstream base onto fork HEAD.
git tag -f "$TAG" HEAD
git push origin "refs/tags/$TAG"
```

If `origin` already has `$TAG` on the upstream commit, the tag push needs `--force`. Ask first; do not force-push unless the user explicitly allows it.

```bash
git push --force origin "refs/tags/$TAG"
```

Then:

- Workflow `Build Windows Linux and macOS Codex CLI` runs on that tag
- GitHub Release `$TAG` gets:
  - `codex-rust-vX.Y.Z-x86_64-pc-windows-msvc.zip`
  - `codex-rust-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz`
  - `codex-rust-vX.Y.Z-aarch64-apple-darwin.tar.gz`
  - `codex-rust-vX.Y.Z-x86_64-apple-darwin.tar.gz`
- `workflow_dispatch` does **not** create a Release

Do not push a tag whose commit lacks `.github/workflows/build-windows-codex.yml` (that is the usual failure when pushing the unmodified upstream tag object).

## CI / publish checklist (must match)

After upgrade, `.github/workflows/build-windows-codex.yml` must:

- [ ] `on.push.tags` is `rust-v*.*.*` (not a branch trigger)
- [ ] `publish-github-release` exists, runs only when `github.ref_type == 'tag'`, and sets `GH_REPO`
- [ ] Windows builds four bins (CLI + code-mode-host + two sandbox helpers)
- [ ] Linux builds `codex` + `codex-code-mode-host`
- [ ] macOS matrix builds arm64 + x86_64 (`codex` + `codex-code-mode-host`)
- [ ] Artifacts uploaded per platform
- [ ] Every file in section C is absent

## Conflict / failure playbook

| Symptom | Action |
|---------|--------|
| Tag missing | `git ls-remote --tags upstream 'rust-v*'` and confirm with user |
| Cherry-pick conflict in tests | Keep upstream tests + PostCompact tests side by side |
| Type errors on `additional_contexts` | Follow current `session_start.rs` / `user_prompt_submit.rs` patterns |
| Workflow missing on tag branch | Re-add section B from previous branch |
| `rust-release.yml` (or other section C files) reappeared | Expected after checkout; delete again (section C) |
| Skills budget cherry-pick fails / files missing | Search the names in section D; patch current crate (`ext/skills` or `core-skills`); update tests and `2%` copy |
| New branch still shows 8_000 / 2% | Stop; budget port did not land |
| Dirty `Cargo.lock` after check | Prefer tag lockfile if no new crates |
| Tag push built the wrong commit / no Release | Tag was still the upstream object; retarget onto fork HEAD and push the tag again (force only with user approval) |
| `rust-release` workflow started on the fork | Section C files came back; delete them, they cannot succeed here |
| Publish job: `failed to run git: not a git repository` | Packing succeeded but `gh` had no repo; set `GH_REPO` on the publish job (do not re-run the old workflow file) |

## Out of scope

- Syncing GitHub “Sync fork” alone does **not** replace this flow (tags/branches are not auto-mirrored forever).
- Desktop App “Computer Use” packaging (x64 missing plugin) is **not** fixed by this skill.
- Do not force-push or rewrite published history without user request.

## Quick command summary

```bash
TAG=rust-v0.150.0
PREV=dev/from-rust-v0.149.1   # adjust
BRANCH=dev/from-rust-v0.150.0

git fetch upstream tag "$TAG" --no-tags
git checkout -b "$BRANCH" "$TAG"
# cherry-pick PostCompact feature/adapt/docs...
# port skills metadata budget (16_000 / 4%) by content if needed
# copy fork CLI publisher; delete section C workflows restored by the tag
cd codex-rs && cargo check -p codex-hooks && cargo check -p codex-core
# commit remaining changes
# wait for user before:
#   git push -u origin "$BRANCH"
#   git tag -f "$TAG" HEAD
#   git push origin "refs/tags/$TAG"
```
