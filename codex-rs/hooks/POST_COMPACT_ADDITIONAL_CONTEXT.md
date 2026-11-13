# PostCompact 上下文注入（additionalContext）

| 项 | 内容 |
|----|------|
| 分支 | `dev/from-rust-v0.148.0` |
| 能力提交 | `fee7872cf3` |
| 基于 tag | `rust-v0.148.0` |
| 仓库（fork） | https://github.com/zcb617/codex |

本文说明 Codex **PostCompact** hook 在上下文压缩（compaction）成功后，如何向模型上下文 **追加** 用户指定内容，以及如何配置、联调与扩展。

---

## 1. 背景与问题

长任务运行过程中，模型上下文可能被 **自动或手动压缩**。压缩后，部分关键指令、项目约定等会从 history 中丢失。

既有机制：

| Hook | 触发时机 | 能否注入上下文 | 问题 |
|------|----------|----------------|------|
| **SessionStart**（`source=compact`） | **下一次用户 turn 开始**（用户再次发提示词时） | 能 | 中途 auto-compact 后，同一轮 agent 续跑 **不会** 等到用户发消息，因此无法及时注入 |
| **PostCompact**（改造前） | **压缩成功后立刻** | 否（仅 stop / systemMessage 等） | 能触发，但不能写回模型上下文 |

需求：在 **PostCompact** 上提供与 SessionStart 类似的 **结构化上下文注入**，使 mid-turn 续跑也能立刻看到注入内容。

---

## 2. 设计结论

### 2.1 做什么

压缩 **成功** 后运行 PostCompact；若 hook stdout 为合法 JSON 且包含：

```json
{
  "hookSpecificOutput": {
    "hookEventName": "PostCompact",
    "additionalContext": "需要注入的文本"
  }
}
```

则将 `additionalContext` 转成一条 **developer** 角色消息，**append** 进 conversation history。

### 2.2 明确不做

| 行为 | 是否支持 | 说明 |
|------|----------|------|
| 纯文本 stdout 当上下文 | **否** | 避免 hook 日志误注入模型；与 SessionStart 不同 |
| 覆盖已有 history | **否** | 仅追加，不 replace / 不 delete |
| 修改压缩摘要本身 | **否** | 注入发生在 **替换 history 之后** |
| 取代 SessionStart(compact) | **否** | 两者并存，职责不同 |

### 2.3 与 SessionStart(compact) 的分工

```text
压缩成功
  ├─ PostCompact          → 立刻 append additionalContext（本功能）
  └─ 排队 SessionStart(source=compact)
        └─ 仅在「下一次用户 turn 开始」时执行
```

| 场景 | 推荐 |
|------|------|
| 长任务 mid-turn auto-compact 后，**同一轮** 继续跑就要带上约定 | **PostCompact** |
| 用户下次发消息时再注入即可 | SessionStart + `matcher: "compact"` |
| 两边都配同一段文案 | 可能 **双注入**，由配置方自行避免 |

---

## 3. 运行时序

以 mid-turn auto-compact 为例：

```text
User turn 进行中
  → 采样 / 工具调用 …
  → token 超限或触发 auto compact
  → PreCompact（可 stop）
  → 执行 compact，替换 history（摘要 + 策略相关 reinject）
  → PostCompact
       → 解析 stdout
       → 若有 additionalContext：record_conversation_items（append developer 消息）
       → 若 continue:false：中止后续（仍先记录已解析到的 context）
  → 同一 turn 内 continue 采样  ← 本次注入已在 history 中
```

手动 `/compact` 或 `Op::Compact`：压缩成功后同样立刻注入；下一次模型请求可见。

---

## 4. 注入语义（给实现与测试对齐）

### 4.1 Append，不是覆盖

```text
[... 压缩后的 history ...]
+ ResponseItem(developer): "<additionalContext>"
```

实现路径：

1. `codex-hooks` 收集 `Vec<String>` additional_contexts  
2. `hook_runtime::record_additional_contexts`  
3. `HookAdditionalContext` → developer 消息  
4. `Session::record_conversation_items` 写入 history  

### 4.2 多 hook / 多段文本

- 多个 PostCompact handler 都返回 `additionalContext` → **按完成结果依次 flatten 后全部 append**  
- 同一 handler 返回一段字符串 → 一条 developer 消息  
- 多次 compact → 每次成功注入都会再 append 一条；旧注入可能在后续 compact 中被摘要掉（取决于压缩策略）

### 4.3 continue:false 与 context

对齐 SessionStart 行为：

- 若 JSON 中同时有 `continue: false` 与 `additionalContext`：  
  **先记录 context，再 stop**  
- stop 后 agent 后续步骤可能中止，但已写入 history 的 context 保留

### 4.4 失败与忽略

| stdout | 结果 |
|--------|------|
| 空 | 无注入，Continue |
| 合法 JSON + additionalContext | 注入 |
| 合法 JSON 无 additionalContext | 无注入（可 systemMessage / continue 等） |
| 看起来像 JSON 但解析失败 | Failed，不注入 |
| **非 JSON 纯文本** | **忽略**（不注入、不 Failed） |
| exit code ≠ 0 | Failed，不注入 |

---

## 5. Hook 协议

### 5.1 配置位置

- 用户级：`~/.codex/hooks.json`（Windows：`%USERPROFILE%\.codex\hooks.json`）  
- 项目级：`<repo>/.codex/hooks.json`  
- 需开启 Codex Hooks 能力，并对 hook 完成 **信任**（与现有 hook 体系一致）

示例 `hooks.json`：

```json
{
  "hooks": {
    "PostCompact": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "python3 /path/to/post_compact_inject.py",
            "statusMessage": "post compact inject"
          }
        ]
      }
    ]
  }
}
```

仅 auto 压缩时注入（matcher 对应 stdin 的 `trigger`）：

```json
{
  "hooks": {
    "PostCompact": [
      {
        "matcher": "auto",
        "hooks": [
          {
            "type": "command",
            "command": "python3 /path/to/post_compact_inject.py"
          }
        ]
      }
    ]
  }
}
```

`matcher` 常见值：`manual` | `auto`（与 trigger 枚举一致，可为正则）。

### 5.2 输入（stdin JSON）

Schema：`codex-rs/hooks/schema/generated/post-compact.command.input.schema.json`

主要字段：

| 字段 | 说明 |
|------|------|
| `session_id` | 会话 / thread id |
| `turn_id` | Codex 扩展：当前 turn id |
| `cwd` | 工作目录 |
| `transcript_path` | transcript 路径，可能为 null |
| `hook_event_name` | 固定 `"PostCompact"` |
| `model` | 模型 slug |
| `trigger` | `"manual"` 或 `"auto"` |
| `agent_id` / `agent_type` | 可选；thread-spawn 子代理内运行时出现 |

### 5.3 输出（stdout JSON）——注入时必须用这个形状

Schema：`codex-rs/hooks/schema/generated/post-compact.command.output.schema.json`

**推荐最小注入输出：**

```json
{
  "hookSpecificOutput": {
    "hookEventName": "PostCompact",
    "additionalContext": "压缩后请继续按 plan.md 执行；测试命令：just test -p codex-core。"
  }
}
```

可选通用字段（与其它 hook 一致）：

| 字段 | 默认 | 说明 |
|------|------|------|
| `continue` | `true` | `false` 时 stop 后续 |
| `stopReason` | null | stop 原因 |
| `systemMessage` | null | 用户可见警告类条目（非 model additionalContext 通道） |
| `suppressOutput` | `false` | 通用字段；当前 PostCompact 不依赖其做注入 |

**约束：**

- `hookSpecificOutput.hookEventName` **必须** 为 `"PostCompact"`  
- 只认 **JSON** 中的 `additionalContext`；`echo "hello"` 这类纯 stdout **不会** 注入  

### 5.4 示例脚本（Python）

```python
#!/usr/bin/env python3
import json
import sys

payload = json.load(sys.stdin)
# 调试日志请写 stderr，避免污染 stdout
print(f"post_compact trigger={payload.get('trigger')}", file=sys.stderr)

context = """## Persistent project notes (re-injected after compact)
- Repo layout: monorepo under codex-rs/
- Prefer: just test -p <crate>
- Do not force-push without explicit approval
"""

print(json.dumps({
    "hookSpecificOutput": {
        "hookEventName": "PostCompact",
        "additionalContext": context,
    }
}))
```

Windows 配置 `command` 时使用可执行解释器路径，例如：

```text
python C:\Users\<you>\.codex\post_compact_inject.py
```

---

## 6. 代码落点（给改协议 / 排障的人）

| 层级 | 路径 | 职责 |
|------|------|------|
| Wire schema | `codex-rs/hooks/src/schema.rs` | `PostCompactCommandOutputWire` + `PostCompactHookSpecificOutputWire` |
| Schema fixture | `codex-rs/hooks/schema/generated/post-compact.command.output.schema.json` | 对外 JSON Schema |
| Parser | `codex-rs/hooks/src/engine/output_parser.rs` | `parse_post_compact` → `additional_context` |
| Event | `codex-rs/hooks/src/events/compact.rs` | 解析、收集 contexts；**忽略 plain stdout** |
| Outcome | `StatelessHookOutcome.additional_contexts` | 返回给 core |
| Runtime | `codex-rs/core/src/hook_runtime.rs` → `run_post_compact_hooks` | `record_additional_contexts` |
| Fragment | `codex-rs/core/src/context/hook_additional_context.rs` | developer 消息封装 |
| 调用方 | `compact.rs` / `compact_remote*.rs` / `compact_token_budget.rs` | 压缩成功后调用 PostCompact |

注入调用（核心一行语义）：

```rust
// run_post_compact_hooks：emit completed 之后
record_additional_contexts(sess, turn_context, outcome.additional_contexts).await;
```

---

## 7. 测试

### 7.1 单元（hooks crate）

`codex-rs/hooks/src/events/compact.rs`：

- `post_compact_additional_context_is_recorded`  
- `post_compact_continue_false_still_records_additional_context`  
- `post_compact_ignores_plain_stdout`（回归：纯文本不注入）  

```bash
cd codex-rs
cargo test -p codex-hooks --lib events::compact
```

### 7.2 集成（core）

`codex-rs/core/tests/suite/hooks.rs`：

- `post_compact_hook_records_additional_context_immediately`：手动 compact 后，下一 turn 的 model input 含 developer 文本  
- `post_compact_hook_injects_context_on_mid_turn_auto_compact`：mid-turn auto compact 后，**同一 turn 续采样** request 含注入文本  

```bash
cd codex-rs
RUST_MIN_STACK=33554432 cargo test -p codex-core --test all post_compact_hook -- --test-threads=1
```

（部分 hook 集成测试在 debug 下栈较深，必要时提高 `RUST_MIN_STACK`。）

### 7.3 Schema

```bash
cd codex-rs
cargo test -p codex-hooks --lib schema::tests
# 若改 wire 类型，重新生成 fixture：
cargo run -p codex-hooks --bin write_hooks_schema_fixtures
```

---

## 8. 构建与使用本分支二进制

本能力 **不在** 上游官方 release / npm 包中，需使用本 fork 分支构建。

### Linux

```bash
git checkout dev/from-rust-v0.148.0
cd codex-rs
cargo build --release --bin codex
# 产物：target/release/codex
```

### Windows（CI）

Workflow：`.github/workflows/build-windows-codex.yml`

- 产出 artifact：`codex-x86_64-pc-windows-msvc`  
- 内含：`codex.exe`、`codex-code-mode-host.exe`（需使用同时编译两者的那次 run；仅编 `codex.exe` 的旧 artifact 会缺 host）  

Actions：https://github.com/zcb617/codex/actions/workflows/build-windows-codex.yml  

---

## 9. 配置与联调检查清单

1. 使用的是 **本分支编译的** `codex` / `codex.exe`，不是旧官方包  
2. Hooks 功能已开启，且 PostCompact command 已 **trusted**  
3. Hook **stdout** 仅为 JSON（日志走 **stderr**）  
4. `hookEventName` 拼写为 `PostCompact`（大小写敏感）  
5. 压缩确实发生：auto（token 压力）或 manual compact  
6. UI / 事件流中可见 PostCompact hook completed；若有 Context 类 entry，说明解析到了 additionalContext  
7. 检查下一发 model request 的 `developer` 输入是否包含注入字符串  

---

## 10. FAQ

**Q: 注入是追加还是覆盖？**  
A: **追加（append）**。不删除、不替换压缩后已有 history。

**Q: 为什么 plain stdout 不能注入？**  
A: PostCompact 常用于日志/旁路逻辑；若 stdout 一律进模型，容易把调试输出灌进上下文。SessionStart 仍支持 plain stdout，语义不同。

**Q: 和 PreCompact 的关系？**  
A: PreCompact 在压缩前，可 stop 压缩；**不支持** 本功能的 additionalContext 注入。注入只在 PostCompact（压缩成功后）。

**Q: SessionStart matcher=compact 还要不要？**  
A: 可选保留。适合「只在用户下次发言时补上下文」。mid-turn 立刻补上下文请用 PostCompact。

**Q: additionalContext 有大小限制吗？**  
A: 走通用 hook context 路径；过大可能被 spill 到文件（与其它 hook additionalContext 一致）。产品侧仍应避免无界超大文本。

**Q: continue:false 后注入还在吗？**  
A: 解析到的 context **会先写入**；再按 stop 处理。是否继续采样取决于 stop 后的 turn 控制逻辑。

---

## 11. 变更摘要（review 用）

| 提交 | 说明 |
|------|------|
| `fee7872cf3` | PostCompact 支持 `hookSpecificOutput.additionalContext`，压缩后立刻 append |
| `4ff2d3adf5` / `23d79b6f31` | Windows CI 产出 `codex.exe` + `codex-code-mode-host.exe` |

协议兼容：旧 PostCompact hook 只输出 `continue` / `systemMessage` 或纯日志时行为与改造前一致；**新增**可选 JSON 字段后才启用注入。

---

## 12. 联系与延伸阅读

- Hook 输出 schema 目录：`codex-rs/hooks/schema/generated/`  
- 相关事件实现：`codex-rs/hooks/src/events/compact.rs`、`session_start.rs`  
- 运行时注入：`codex-rs/core/src/hook_runtime.rs`  

文档版本：与分支 `dev/from-rust-v0.148.0` 上 PostCompact 注入实现同步。若协议字段有演进，以 schema fixture 与代码为准更新本文。
