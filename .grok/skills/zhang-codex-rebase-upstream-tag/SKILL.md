---
name: zhang-codex-rebase-upstream-tag
description: >
  Rebase this Codex fork's custom commits onto a new upstream rust-vX.Y.Z
  release tag, keep the fork CLI publisher, delete upstream release workflows
  that the tag brings back, then
  push the branch and retarget rust-vX.Y.Z onto the fork HEAD of origin
  (zcb617/codex) so Actions builds and attaches binaries to a GitHub Release.
  Never push to upstream openai/codex. Do not ask whether to push. Use when
  the user says upstream has a new tag (e.g. rust-v0.146.0), wants to
  "基于上游新TAG建分支", "合并本地修改", "像之前升0.145一样", "发版",
  "打tag发release", "同步", or runs /zhang-codex-rebase-upstream-tag.
---

# Rebase custom fork work onto a new upstream rust tag

Automate the recurring upgrade and CLI publish for **this** Codex fork (`zcb617/codex` / `my_codex`):

1. Base on upstream `rust-vX.Y.Z`
2. Create `dev/from-rust-vX.Y.Z`
3. Port local custom commits (PostCompact + skills metadata budget + daemon auto-start stays off + fork CLI publisher)
4. Delete the upstream release workflows that the new tag brings back
5. Locally verify files, constants, and workflows, then commit
6. Push the branch and retarget `rust-vX.Y.Z` onto **fork HEAD** of **origin only** (`zcb617/codex`) so Actions publishes the GitHub Release. Do not ask. Never push to `upstream`.

Always answer the user in **中文**.

## Local machine policy（磁盘）

本机磁盘紧。发布时本机只做静态核对，不运行 `cargo check`、编译、测试或打包；Rust 类型检查、编译和打包交给 push 之后的 GitHub Actions 多平台构建。本条覆盖仓库 `AGENTS.md` 里「改完跑 `just test`」的要求。是否运行测试，以实际工作流和运行结果为准，不要把编译称为测试通过。

允许：

- `git` / `rg` / `test -f` 核对文件、常量和 workflow

禁止（用户在本轮对话里明确要求覆盖之前一律不准）：

- `cargo check`、`cargo build`、`--release`、跑编出来的 `codex`
- `cargo test`、`just test`、`cargo nextest`、`just bench`
- 工作区级 `just fix` / 全量 clippy
- 本机打包 CLI

## Prerequisites / remotes

- Working tree should be clean before starting (stash or commit unrelated work).
- Remotes:
  - `origin` → user fork **only**: `github.com:zcb617/codex.git` (or `https://github.com/zcb617/codex.git`). This is the only remote that may receive `git push`.
  - `upstream` → `https://github.com/openai/codex.git` (add if missing). **fetch only**. Never `git push upstream`, never `--force` to upstream, never push a branch or tag there.
- Before any push, abort unless `git remote get-url --push origin` is exactly the fork URL above.
- Source branch for custom work is usually the previous release branch, e.g. `dev/from-rust-v0.149.1` (or whatever is currently checked out / latest `dev/from-rust-v*`).

## Inputs

Ask only if not already stated:

- **New tag**: e.g. `rust-v0.150.0` (must exist on upstream)
- **Source branch** (optional): previous customized branch; default = current `dev/from-rust-v*` that tracks the last done upgrade

After the local commit, **push to origin immediately**. Do not ask “要不要推”. The user already wants GitHub Actions to run.

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
  - **macOS** matrix `macos-latest`: `aarch64-apple-darwin` + `x86_64-apple-darwin` → `codex` + `codex-code-mode-host`. Job `timeout-minutes` is **240** (x86_64 is cross-compiled on arm64 runners and can exceed 2h)
- After copying the previous publisher, set every `dtolnay/rust-toolchain@` ref to the `channel` in `codex-rs/rust-toolchain.toml`, and pass that file's `components`. The action installs the `@ref` compiler; it does not read `rust-toolchain.toml`. On the macOS job, also run `rustup target add "${{ matrix.target }}" --toolchain <channel>` so the x86_64 cross std is on that same compiler. A mismatch previously installed 1.95.0 while 0.164.0 required 1.99.1, and the x86_64 job died with `can't find crate for std`.
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

### E. 后台服务默认关闭（`daemon_auto_start`）

直接运行 `codex` 必须还能启动。Linux、Windows、macOS 共用这一个开关。

上游从 **rust-v0.157.0** 起把 `Feature::DaemonAutoStart`（key `daemon_auto_start`）改成 `Stage::Stable` 且 `default_enabled: true`。默认打开后，第一次启动会进入 `Daemon::start`，先要求旁边有完整官方包，没有就退出并报 `this CLI has no complete local package`。`~/.local/bin` 里只有 `codex` 和 `codex-code-mode-host` 时会中招。0.156 以及更早是实验功能且默认关闭，不会走这一步。

每个新 tag 都要核对，按**内容**改，不要按旧 SHA 挑：

- 搜 `daemon_auto_start` / `Feature::DaemonAutoStart`。当前在 `codex-rs/features/src/lib.rs`，以后换文件也要找到这份 `FeatureSpec`
- `default_enabled` 必须是 **`false`**
- 上游若把 stage 改成 `Stable` 且默认打开，改回 0.156 的实验项并且默认关闭：

```rust
stage: Stage::Experimental {
    name: "Automatically start the background server",
    menu_description: "Use the shared local server for new, resumed, and forked sessions. Takes effect next launch.",
    announcement: "Automatic background server startup can now be enabled from /experimental.",
},
default_enabled: false,
```

上游如果已经是 `default_enabled: false`，保持即可，不要再改成打开。功能被上游删掉时，不要为了这项把它加回去。

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
| `daemon_auto_start` 保持默认关闭（section E） | 把上游默认打开的后台服务原样留下 |
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
# Then port section D (skills budget) and section E (daemon_auto_start default off)
# if cherry-pick of those changes fails or the feature spec moved
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

Cherry-pick 带过来的旧提交说明作废，最终说明只按第 7 步。

### 5. Cargo.lock hygiene

- Prefer the **tag’s** `codex-rs/Cargo.lock` unless source changes require real dependency updates.
- If an old adapt commit dragged in a previous minor lockfile, restore:

```bash
git checkout rust-vX.Y.Z -- codex-rs/Cargo.lock
```

### 6. Verify

本机只核对文件、常量和发布工作流；不运行 `cargo check`。GitHub Actions 的 Windows、Linux 和两个 macOS 目标构建负责对应平台的编译和 Rust 类型检查。

After porting, confirm skills budget landed:

```bash
rg -n 'DEFAULT_SKILL_METADATA_CHAR_BUDGET|SKILL_METADATA_CONTEXT_WINDOW_PERCENT|2% skills context budget' \
  --glob '*.rs' codex-rs
```

Expect `16_000` and `4` (or larger if upstream already exceeded those). Fail the upgrade if the new branch still has `8_000` / `2` as the catalog defaults.

Confirm the background server stays off by default (section E). Linux、Windows、macOS 都看这一处：

```bash
rg -n -A 15 'DaemonAutoStart|daemon_auto_start' --glob '*.rs' codex-rs/features
```

紧挨着 `id: Feature::DaemonAutoStart` 的 `default_enabled` 必须是 `false`。仍是 `true` 就停止升版，改完再查。不要只改 Linux。

Confirm publishers:

```bash
test -f .github/workflows/build-windows-codex.yml
test ! -e .github/workflows/rust-release.yml
rg -n 'rust-v\*\.\*\.\*|publish-github-release|GH_REPO' .github/workflows/build-windows-codex.yml
```

Fail the upgrade if `GH_REPO` is missing from the publish job. Without it, packing succeeds then `gh release` dies with `not a git repository`.

本机静态核对到此结束。多平台编译和打包见 Local machine policy。

### 7. Commit

移植和检查都做完后，把新分支上、上游 tag 之上的提交收成 **一条**。分支已经推过远程时，不要改写历史，先停下来问用户。

```bash
git reset --soft "$TAG"
git commit -F /tmp/upgrade-msg.txt
rm -f /tmp/upgrade-msg.txt
```

`/tmp/upgrade-msg.txt` 只能有下面这两段，版本号换成这次的 tag。不要标题以外的英文，不要正文解释。B 的多平台 CLI 发布、删除上游工作流、技能、`GH_REPO`、rusty-v8、`Cargo.lock`、本机检查，都要做，但都不写进说明：

```text
同步上游 rust-vX.Y.Z

- 压缩完成后立即注入 PostCompact additionalContext
- skills 元数据上下文预算提高到 4% / 16000
- 后台服务保持默认关闭
```

这三条是当前要写进说明的强化功能（A、D、E）。以后新增一项强化功能，只在这里加一行功能名，同时补进 Custom content。没移植成功的功能不要写进说明。

### 8. Publish to origin (same turn; do not ask)

The fetched `rust-vX.Y.Z` points at the **upstream** commit. Actions must build **fork HEAD** (PostCompact + publisher workflow). Retarget the tag onto HEAD, then push **origin only**.

```bash
ORIGIN_URL=$(git remote get-url --push origin)
case "$ORIGIN_URL" in
  git@github.com:zcb617/codex.git|https://github.com/zcb617/codex.git) ;;
  *) echo "ABORT: origin is $ORIGIN_URL"; exit 1 ;;
esac

BRANCH=dev/from-rust-vX.Y.Z
TAG=rust-vX.Y.Z

git push -u origin "$BRANCH"

# Move the local tag from upstream base onto fork HEAD.
git tag -f "$TAG" HEAD
git push origin "refs/tags/$TAG"
```

If `origin` already has `$TAG` on another commit, force-update **that origin tag** so Actions builds fork HEAD:

```bash
git push --force origin "refs/tags/$TAG"
```

Force is allowed only for retargeting `rust-v*` on **origin**. Never `git push upstream`. Never `git push --force upstream`. Never any ref to `openai/codex`.

Then:

- Workflow `Build Windows Linux and macOS Codex CLI` runs on that tag
- GitHub Release `$TAG` gets:
  - `codex-rust-vX.Y.Z-x86_64-pc-windows-msvc.zip`
  - `codex-rust-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz`
  - `codex-rust-vX.Y.Z-aarch64-apple-darwin.tar.gz`
  - `codex-rust-vX.Y.Z-x86_64-apple-darwin.tar.gz`
- `workflow_dispatch` does **not** create a Release

Do not push a tag whose commit lacks `.github/workflows/build-windows-codex.yml` (that is the usual failure when pushing the unmodified upstream tag object).

### 9. Report after the origin push

Tell the user in 中文:

- New branch name and base tag
- Commits on top of the tag (`git log --oneline rust-vX.Y.Z..HEAD`)
- `git describe --tags --always HEAD` after retarget (should match fork HEAD)
- 本机静态核对结果，以及 GitHub Actions 四个平台的构建状态；测试只在工作流实际运行时报告
- origin 分支、tag SHA，以及 GitHub Actions run（`gh run list --repo zcb617/codex --limit 3`）

## CI / publish checklist (must match)

After upgrade, `.github/workflows/build-windows-codex.yml` must:

- [ ] `on.push.tags` is `rust-v*.*.*` (not a branch trigger)
- [ ] `publish-github-release` exists, runs only when `github.ref_type == 'tag'`, and sets `GH_REPO`
- [ ] Windows builds four bins (CLI + code-mode-host + two sandbox helpers)
- [ ] Linux builds `codex` + `codex-code-mode-host`
- [ ] macOS matrix builds arm64 + x86_64 (`codex` + `codex-code-mode-host`); macos job `timeout-minutes` is 240
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
| `daemon_auto_start` 仍是 `default_enabled: true` | 按 section E 改回默认关闭；三端共用这一处，不要只改一个系统 |
| Dirty `Cargo.lock` without dependency changes | Prefer tag lockfile if no new crates |
| Tag push built the wrong commit / no Release | Tag was still the upstream object; retarget onto fork HEAD and `git push --force origin "refs/tags/$TAG"` (origin only) |
| `rust-release` workflow started on the fork | Section C files came back; delete them, they cannot succeed here |
| Publish job: `failed to run git: not a git repository` | Packing succeeded but `gh` had no repo; set `GH_REPO` on the publish job (do not re-run the old workflow file) |
| macOS x86_64 job cancelled at 2h, rustc still running, no compile error | Cross-compile on `macos-latest` (arm64). Keep macos `timeout-minutes: 240`. Re-run of the old SHA keeps the old 120m cap; push the timeout commit and retarget the tag |
| macOS x86_64: `can't find crate for std` / target may not be installed | Publisher `@ref` is not `codex-rs/rust-toolchain.toml` `channel`, or the cross target was installed on a different compiler. Pin `dtolnay/rust-toolchain@<channel>`, then `rustup target add` that same toolchain. New commit; do not amend the upgrade commit. Force-update origin `rust-v*` only |

## Out of scope

- Syncing GitHub “Sync fork” alone does **not** replace this flow (tags/branches are not auto-mirrored forever).
- Desktop App “Computer Use” packaging (x64 missing plugin) is **not** fixed by this skill.
- Never push any ref to `upstream` / `openai/codex`. Fetch from upstream is allowed.
- Force-updating `rust-v*` on **origin** so Actions builds fork HEAD is part of this flow. Do not rewrite unrelated published branches.
- Do not compile, test, or package on this machine; see Local machine policy.

## Quick command summary

```bash
TAG=rust-v0.150.0
PREV=dev/from-rust-v0.149.1   # adjust
BRANCH=dev/from-rust-v0.150.0

git fetch upstream tag "$TAG" --no-tags
git checkout -b "$BRANCH" "$TAG"
# cherry-pick PostCompact feature/adapt/docs...
# port skills metadata budget (16_000 / 4%) by content if needed
# keep daemon_auto_start default_enabled false (section E)
# copy fork CLI publisher; delete section C workflows restored by the tag
# 本机只核对文件、常量和发布工作流；四个平台的编译和类型检查由 GitHub Actions 完成
# 收成一条提交：同步上游 $TAG，外加 A、D、E 三条强化功能，不要写 CLI 发布
# origin 必须是 zcb617/codex；禁止 git push upstream
git push -u origin "$BRANCH"
git tag -f "$TAG" HEAD
git push origin "refs/tags/$TAG"
```
