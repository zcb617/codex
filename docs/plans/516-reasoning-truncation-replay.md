# 516 推理截断重放开发方案

状态：已确认口径，尚未改业务代码。本文是实施说明。按文中的文件、函数和步骤修改即可，不要再设计另一套续接方式。

行号以 2026-10-27 工作区为准，定位时以函数名为准。参考实现在 `/var/work/codex_reason/CodexCont`，只对照行为，不把该仓库的代码搬进本仓库。

## 1. 业务目标

模型一次响应的 `usage.reasoning_output_tokens` 落在截断指纹上时，这次响应里的正文和工具调用不能进入当前对话。CLI 立刻重新发一次请求。新请求带上完整输入、已经产生的 reasoning，以及一条只存在于这次请求里的 commentary。新请求不带被截断响应的 `previous_response_id`。

用户仍看到同一个回合。reasoning 照常逐段出现。被截断那一轮的正文不显示，工具不执行。自然结束或达到上限的那一轮，才显示正文并执行工具。达到上限时如果仍是指纹，停止重放，并交出该轮已经产生的正文和工具调用。

重放成功后，后续工具结果和用户下一句话继续使用最后一轮干净响应的 id，恢复现有增量对话。

## 2. 已核实事实

| 事实 | 出处 | 证明的内容 |
| --- | --- | --- |
| 指纹是 `tokens >= 516` 且 `(tokens + 2) % 518 == 0`。第一档 516，其后 1034、1552、2070、2588。 | `CodexCont/middleware/codex.py` 的 `is_truncation_pattern`、`tier_n`、`should_continue` | 判定公式 |
| 默认只在 `1 <= n <= 6` 时续写。`max_continue = 3` 表示第 1、2、3 轮命中后还可以再发，第 4 轮再命中则停止。 | `CodexCont/config.example.toml` 的 `[continue]`；`proxy.py` 中 `round_no <= cont.max_continue` | 次数和档位上限 |
| 还要求终态事件已到，且本轮最后一条 reasoning 有 `encrypted_content`。没有加密内容、超出档位或超出次数时，不再重放，并交出本轮暂存的正文和工具调用。 | `CodexCont/middleware/proxy.py` 的 `do_continue` 与随后的 flush | 停止后的交付规则 |
| 续写输入是原始 input、本轮 reasoning、一条 `phase: "commentary"` 的助手消息，正文固定为 `Continue thinking...`。暂存的 message 和 function_call 不进入该输入。 | `codex.py` 的 `commentary_message`；`proxy.py` 的 `replay_tail`；`tests/test_ws_incremental.py` | 重放载荷 |
| 只有续写请求删除 `previous_response_id`。第一轮和续写成功后的下一轮仍保留调用方原来的 id。 | `codex.py` 的 `build_round_payload`；`app.py` 约 131–137 行注释；`test_ws_incremental.py` 中第一轮保持 `resp_seed`，续写后下一轮使用 `resp_hidden` | 只作废被截断的响应 |
| `prompt_cache_key` 不删除。前缀缓存认这个键和逐 token 相同的前缀，不认 `previous_response_id`。 | `build_round_payload` 只 `pop("previous_response_id")`；CLI `ModelClient::prompt_cache_key`，默认键是会话 id | 重放不关闭前缀缓存 |
| CLI 请求 `store: false`。`previous_response_id` 只在当前 WebSocket 上有效。下一次请求能增量，是因为 `prepare_websocket_request` 读到了 `LastResponse.response_id`。 | `core/src/client.rs` 约 1119 行 `store: false`；`prepare_websocket_request`；`map_response_events` 在 `Completed` 时发送 `LastResponse` | 不丢弃 receiver，下一次就会接上被截断的 id |
| 若截断轮只有 reasoning，再追加 commentary，现有增量判断会把 commentary 当成增量，并带上被截断的 id。 | `get_incremental_items`：基线是上次请求 input 加上 `items_added` | 必须主动丢掉 `LastResponse`，不能靠输入不一致碰运气 |
| 工具在 `OutputItemDone` 时就已经执行，此时还没有 usage。 | `session/turn.rs` 的 `ResponseEvent::OutputItemDone` 调用 `handle_output_item_done`；`stream_events_utils.rs` 的 `handle_output_item_done` 立即 `record_completed_response_item` 并启动工具 | 正文和工具必须先攒住 |
| reasoning 项的 `encrypted_content` 在 OpenAI 路径上会保留。请求已经包含 `reasoning.encrypted_content`。 | `ResponseItem::Reasoning`；`client.rs` 约 1075 行 `include` | 重放材料已有，不必改 include |
| HTTP 传输每次发送完整 input，不使用 `previous_response_id`。WebSocket 失败后会退回 HTTP。 | `ModelClientSession::stream` | 判定和攒消息两条传输共用；丢掉 response id 只影响 WebSocket |
| 本仓库已有的 compaction 是上下文压缩，不是这次的截断重放。 | `core/src/compact_remote_v2.rs` 等 | 不修改压缩流程 |

## 3. 稳定项、变化项、既有基础设施

| 分类 | 内容 | 归属 |
| --- | --- | --- |
| 稳定项 | 指纹公式、档位、次数、commentary 文本、最后一条 reasoning 必须有非空 `encrypted_content`、被截断轮的正文和工具调用不落历史 | 新建的纯判定模块 |
| 变化项 | WebSocket 是否带 `previous_response_id` | 只在 `ModelClientSession` 丢掉本轮 `LastResponse` |
| 既有基础设施 | WebSocket 连接、`prompt_cache_key`、历史 `clone_history`、工具执行、上下文压缩、HTTP 退回 | 只复用现有入口，不改其语义 |

`prompt_cache_key`、连接复用、`store: false`、压缩和工具运行时都不重做。

## 4. 职责归属

| 业务职责 | 主要责任层 | 依据 | 协作层 | 明确不属于 |
| --- | --- | --- | --- | --- |
| 判断这一轮是否重放 | `reasoning_truncation` | 规则只依赖 token 数、轮次、最后一条 reasoning 的加密内容 | 采样循环传入这三个值 | WebSocket、历史、UI |
| 攒住正文和工具调用，决定写入还是丢弃 | `try_run_sampling_request` | 只有这里同时看到流事件和终态 usage | 命中后调用历史记录和工具执行的现有函数 | 传输层 |
| 重放请求不接被截断的响应 | `ModelClientSession::discard_websocket_continuation` | 只有会话对象持有 `last_response_rx` | 采样循环在发起下一次 `stream` 前调用 | 不改 `prepare_websocket_request` 的一般增量规则 |
| 组重放输入 | `run_sampling_request` | 这里已经负责每轮 `clone_history` 和 `build_prompt` | 判定模块提供 commentary 项 | 不把 commentary 写入 rollout |
| 开关 | `features` | 本仓库用 Feature 做可关闭行为 | 采样循环读取 | 不新增配置文件格式 |

## 5. 行为规格

常量放在判定模块，不要散落到调用处：

```text
TRUNCATION_STEP = 518
MIN_N = 1
MAX_N = 6
MAX_CONTINUE = 3
MARKER_TEXT = "Continue thinking..."
```

`reasoning_tokens` 使用 `TokenUsage.reasoning_output_tokens`。`None` usage 视为没有命中。

`n = (tokens + 2) / 518`，仅当 `tokens >= 516` 且 `(tokens + 2) % 518 == 0`。

一轮从 1 开始。命中且 `round_no <= MAX_CONTINUE` 时重放。因此最多 1 次原始请求加 3 次重放，共 4 次上游请求。

最后一条 `ResponseItem::Reasoning` 的 `encrypted_content` 必须是 `Some` 且非空。没有 reasoning，或最后一条没有加密内容，则不重放。

commentary 项使用现有类型，不新增协议变体：

```rust
ResponseItem::Message {
    encrypted_content: None,
    status: None,
    id: None,
    role: "assistant".to_string(),
    content: vec![ContentItem::OutputText {
        annotations: None,
        logprobs: None,
        text: "Continue thinking...".to_string(),
    }],
    phase: Some(MessagePhase::Commentary),
    internal_chat_message_metadata_passthrough: None,
}
```

这条消息只附加在下一次重放请求的 input 末尾。不调用 `record_completed_response_item`，不写入 rollout，不发给 TUI 作为助手正文。

被截断轮已经通过现有路径写入历史的 reasoning 保留。它们是重放输入的一部分。`clone_history` 会带上它们，不要再手工拼一份。

被截断轮的用量不调用 `record_token_usage_info`，也不发送 `RawResponseCompleted`。上下文长度以最终被接受那一轮的服务端 usage 为准，其中的 `cached_input_tokens` 仍是前缀缓存结果。不要把 CodexCont 对下游伪装“只有第一轮 input”的 usage 搬进 CLI。

取消、邮箱抢占、流错误或没有 `Completed` 时，不重放，沿用现有中断和错误路径。已经攒住但尚未写入的正文和工具调用直接丢弃。已经写入的 reasoning 保留。

功能关闭时，采样循环与现在完全一致，包括 `OutputItemDone` 立即执行工具。

## 6. 调用链

```text
run_sampling_request
  round = 1
  loop:
    input = 首轮使用调用方传入的 input；之后 clone_history()
    若 round > 1，在 input 末尾追加 commentary，不写历史
    build_prompt
    try_run_sampling_request(prompt, round)
      stream()
        WebSocket: prepare_websocket_request
        HTTP: 完整 input
      reasoning 事件立即走现有记录和展示
      正文、工具调用及其 delta 只进入本轮缓冲区
      Completed:
        判定模块给出 Replay 或 Accept
        Replay:
          丢弃缓冲区
          discard_websocket_continuation
          不记录本轮 usage
          返回 ReplayTruncation
        Accept:
          按到达顺序把缓冲区交给现有 handle_output_item_done
          记录本轮 usage
          返回 Finished
    收到 ReplayTruncation: round += 1，继续 loop
    收到 Finished: 返回给现有回合循环
```

重放成功后，下一次普通采样不再追加 commentary。WebSocket 上一次 `Completed` 留下的是干净响应的 `LastResponse`，工具结果仍按今天的增量规则发送。

## 7. 逐文件改动

### 7.1 新建 `codex-rs/core/src/reasoning_truncation.rs`

职责：唯一的指纹和上限规则。没有 I/O。

新增：

```rust
pub(crate) enum TruncationDecision {
    Replay,
    Accept { still_truncated: bool },
}

pub(crate) fn decide(
    reasoning_output_tokens: Option<i64>,
    round_no: u32,
    last_reasoning_has_encrypted_content: bool,
) -> TruncationDecision

pub(crate) fn commentary_marker() -> ResponseItem
```

`decide` 的顺序必须是：

1. `reasoning_output_tokens` 不是指纹：`Accept { still_truncated: false }`。
2. 是指纹，但最后一条 reasoning 没有非空 `encrypted_content`：`Accept { still_truncated: true }`。
3. 是指纹，且 `round_no > MAX_CONTINUE`：`Accept { still_truncated: true }`。
4. 是指纹，但 `tier_n < MIN_N` 或 `tier_n > MAX_N`：`Accept { still_truncated: true }`。
5. 其余：`Replay`。

`still_truncated` 只用于日志，不改变 Accept 后的交付。两种 Accept 都要放出缓冲区。

同文件写 `#[cfg(test)]` 单元测试，至少覆盖：

- `None`、`0`、`515`、`517`、`518` 不重放。
- `516`、`1034`、`1552`、`2070`、`2588` 在第 1 轮且有加密内容时重放。
- `516` 在第 4 轮 Accept，且 `still_truncated == true`。
- `3106`（`n = 6`）第 1 轮重放；`3624`（`n = 7`）Accept。
- 指纹命中但 `last_reasoning_has_encrypted_content == false` 时 Accept。
- `commentary_marker()` 的 role、phase、文本与上面的结构一致。

### 7.2 修改 `codex-rs/core/src/lib.rs`

在现有 `mod client;` 附近增加：

```rust
mod reasoning_truncation;
```

不导出为公共 API。

### 7.3 修改 `codex-rs/features/src/lib.rs`

在 `Feature` 枚举的 Stable 区增加：

```rust
/// 516 推理截断时重放完整请求，不接被截断响应。
ReasoningTruncationReplay,
```

在 `FEATURES` 的 Stable 区增加：

```rust
FeatureSpec {
    id: Feature::ReasoningTruncationReplay,
    key: "reasoning_truncation_replay",
    stage: Stage::Stable,
    default_enabled: true,
},
```

关闭方式是配置里的 `features.reasoning_truncation_replay = false`。不要新增另一个配置结构。枚举增加后，若有穷尽匹配编译失败，只把该变体接上，不要给它增加采样以外的行为。

### 7.4 修改 `codex-rs/core/src/client.rs`

在 `impl ModelClientSession` 增加：

```rust
pub(crate) fn discard_websocket_continuation(&mut self, reason: &'static str) {
    self.websocket_session.last_response_rx = None;
    self.websocket_session.continuation_reset_reason = Some(reason);
}
```

调用时 `reason` 固定为 `"reasoning_truncation_replay"`。

效果：下一次 `prepare_websocket_request` 得到 `NonIncrementalReason::NoPreviousResponse`，`stream_responses_websocket` 发送完整 input，且 `previous_response_id` 为 `None`。`prompt_cache_key` 仍由 `build_responses_request` 按会话 id 填入。

不要在这个方法里调用 `WebsocketSession::reset`。`reset` 会拆掉当前连接。重放要留在同一条 WebSocket 上。

不要改 `get_incremental_items`。普通轮次的增量规则保持不变。

### 7.5 修改 `codex-rs/core/src/client_tests.rs`

在 `websocket_continuation_reports_unavailable_response_state` 附近新增测试：

1. 构造与现有测试相同的 `ModelClient`、请求和 `LastResponse { response_id: "truncated-response", items_added: vec![] }`。
2. 把 receiver 放进 `session.websocket_session.last_response_rx`。
3. 调用 `discard_websocket_continuation("reasoning_truncation_replay")`。
4. 断言 `prepare_websocket_request(&request)` 的错误是 `NonIncrementalReason::NoPreviousResponse`。
5. 断言 `continuation_reset_reason == Some("reasoning_truncation_replay")`。
6. 再放入一个新的 `LastResponse { response_id: "clean-response", ... }`，断言下一次 `prepare_websocket_request` 成功且 `response_id` 是 `"clean-response"`。这一步证明只作废被截断的 id，干净响应仍能增量续接。

### 7.6 修改 `codex-rs/core/src/session/turn.rs`

这是唯一编排重放的文件。

#### 返回值

在 `try_run_sampling_request` 附近新增私有枚举：

```rust
enum SamplingAttempt {
    Finished(SamplingRequestResult),
    ReplayTruncation,
}
```

`try_run_sampling_request` 的成功返回从 `CodexResult<SamplingRequestResult>` 改为 `CodexResult<SamplingAttempt>`。错误返回不变。

#### `run_sampling_request`

现有函数在约 1756 行的 `loop` 里组 prompt，并在 `try_run_sampling_request` 返回 `Ok` 时立刻返回给回合循环。

改成：

1. 在该 `loop` 外增加 `let mut truncation_round: u32 = 1`。
2. 首轮继续使用 `initial_input.take()`。从第二轮起继续走现有的 `sess.clone_history().await.for_prompt(...)`。
3. 当 `truncation_round > 1` 时，在 `attach_to_prompt` 之后、`build_prompt` 之前，对 `prompt_input` `push(reasoning_truncation::commentary_marker())`。
4. 把 `truncation_round` 传给 `try_run_sampling_request`。
5. `Ok(SamplingAttempt::Finished(output))` 时保持今天的 `return Ok((output, original_input.unwrap_or(prompt.input)))`。
6. `Ok(SamplingAttempt::ReplayTruncation)` 时不要进入 `handle_response_stream_error`，不要增加 `retry_state`，不要调用 `record_sampling_retry`。执行 `truncation_round += 1` 后 `continue` 这个组 prompt 的 loop。
7. `Err` 仍走现有重试、上下文超限和用量限制分支。

commentary 不在 `clone_history` 的结果里。下一轮如果不再重放，就不会再追加。

#### `try_run_sampling_request`

增加参数 `truncation_round: u32`。

在现有局部变量旁增加：

```rust
let replay_enabled = turn_context
    .config
    .features
    .enabled(Feature::ReasoningTruncationReplay);
let mut withheld_non_reasoning: Vec<PendingNonReasoning> = Vec::new();
```

`PendingNonReasoning` 放在 `turn.rs` 私有位置，字段至少包括：

- `item: ResponseItem`，在 `OutputItemDone` 时填入。
- `added: bool`，表示是否收到过 `OutputItemAdded`。
- `active_turn_item: Option<TurnItem>`，只在功能关闭时使用；功能开启时保持 `None`。
- `tool_diff_consumer`，功能关闭时保持现有行为；功能开启时先不创建，等到 Accept 后由现有 `handle_output_item_done` 处理最终 item。

功能开启时，非 reasoning 的增量文本不向前端吐出。这样被丢弃的正文不会先显示再撤回。代价是自然结束那一轮的正文要等 `Completed` 之后一次交付。reasoning 的现有流式展示不改。

事件处理按下面改，只包住非 reasoning：

`OutputItemAdded`：

- `ResponseItem::Reasoning` 走现有分支。
- 其他 item：功能关闭时走现有分支。功能开启时只记录“已出现”，不调用 `emit_turn_item_started`，不创建工具 diff consumer。

`OutputTextDelta` 和 `ToolCallInputDelta`：

- 功能关闭时走现有分支。
- 功能开启且当前 active item 不是 reasoning 时，直接 `continue`。不要解析、不要发事件。

`OutputItemDone`：

- `ResponseItem::Reasoning` 走现有 `handle_output_item_done`。它会记录 reasoning 并展示。
- 其他 item：功能关闭时走现有路径，包括立即执行工具。
- 功能开启时，把完成的 `ResponseItem` 推入 `withheld_non_reasoning`，不要调用 `handle_output_item_done`，不要 `in_flight.push_back`，不要更新 `last_agent_message`，不要把 call id 放入 `analytics_tool_call_ids`。

`Completed`：

把现有约 3177 行开始的记账拆开。

1. 用本轮已记录的最后一条 reasoning 计算 `last_reasoning_has_encrypted_content`。采样循环里增加 `let mut last_reasoning_encrypted = false`。reasoning 的 `OutputItemDone` 中，若该项 `encrypted_content` 为非空 `Some`，设为 `true`，否则设为 `false`。只认最后一条，后一条空值会覆盖前一条。
2. 功能开启时调用 `reasoning_truncation::decide(token_usage.as_ref().map(|usage| usage.reasoning_output_tokens), truncation_round, last_reasoning_encrypted)`。功能关闭时直接当作 `Accept { still_truncated: false }`。
3. `Replay`：
   - 清空 `withheld_non_reasoning`。
   - `client_session.discard_websocket_continuation("reasoning_truncation_replay")`。
   - 不调用 `record_observed_response_completed`。
   - 不调用 `record_token_usage_info`。
   - 不发送本轮 `track_code_mode_tool_call`。
   - 不设置 `should_emit_token_count`。
   - `break Ok(SamplingAttempt::ReplayTruncation)`。
4. `Accept`：
   - 按缓冲区顺序，对每一项调用现有 `handle_output_item_done`。`previously_active_item` 传 `None`，让现有函数补发 started 和 completed。
   - 把返回的 `tool_future` 推入 `in_flight`，合并 `needs_follow_up` 和 `last_agent_message`。
   - 然后执行今天 `Completed` 分支里的 analytics、`record_observed_response_completed`、`record_token_usage_info`、`end_turn` 处理。
   - `break Ok(SamplingAttempt::Finished(...))`。

Accept 后不要再调用 `discard_websocket_continuation`。`map_response_events` 已经把这次 `Completed` 的 id 放进 `last_response_rx`，它就是下一轮增量的锚点。

流在 `Completed` 前结束、取消或报错时，丢弃 `withheld_non_reasoning`，返回现有错误或现有 `needs_follow_up`。不要把未终态的正文和工具调用补写入历史。

`draining_for_mailbox` 保持现有语义。邮箱抢占若在 `Completed` 前结束本轮，不重放。不要为了重放推迟邮箱。

### 7.7 不修改的文件

| 文件或目录 | 原因 |
| --- | --- |
| `/var/work/codex_reason/CodexCont` | 只作行为对照 |
| `codex-rs/core/src/compact_remote_v2.rs` 及 compaction 调用方 | 压缩不是截断重放 |
| `prepare_websocket_request`、`get_incremental_items` | 普通增量规则不变 |
| `build_responses_request` 的 `include` 和 `prompt_cache_key` | 已经会带加密 reasoning 和会话缓存键 |
| TUI、app-server 协议、SDK | 不新增事件。被接受的正文仍走现有 turn item |
| `docs/config.md` 等用户文档 | FeatureSpec 已表达开关，本方案不扩写用户手册 |

## 8. 日志

重放和因上限停止时各打一条 `tracing::info`，字段只包括 `truncation_round`、`reasoning_output_tokens`、`decision`（`replay` 或 `accept`）、`still_truncated`。不要记录 reasoning 正文、加密内容、response id 或用户输入。

## 9. 验收

本机只允许 `cargo check -p`，不允许 `cargo test`、`cargo build` 或打包。检查完成后删除 `codex-rs/target`。行为测试随授权推送后由 GitHub Actions 执行；本方案的提交本身不推送。

实施者在本机执行：

```bash
cargo check -p codex-core
cargo check -p codex-features
rm -rf codex-rs/target
```

若 `cargo check` 改脏 `codex-rs/Cargo.lock`，把 lock 恢复到检查前，不要提交 lock 抖动。

单元测试必须断言：

| 用例 | 预期 |
| --- | --- |
| 516、1034、1552、2070、2588，第 1 轮，有加密内容 | `Replay` |
| 515、517、第 4 轮的 516、`n = 7`、无加密内容 | `Accept` |
| 丢掉 `truncated-response` 后再放入 `clean-response` | 前者不能续接，后者可以 |
| commentary | 只有指定文本和 `MessagePhase::Commentary`，没有 id |

推送后的集成测试至少要有一个采样流夹具，顺序为：reasoning `OutputItemDone`（带非空 `encrypted_content`）、function call `OutputItemDone`、`Completed { reasoning_output_tokens: 516 }`、下一次请求的 reasoning 和最终正文、`Completed { reasoning_output_tokens: 17 }`。断言：

- 第一次 function call 没有执行，历史里没有它。
- 第一次 `Completed` 后没有 `RawResponseCompleted`，也没有 TokenCount 记账。
- 第二次请求的 input 含已记录 reasoning 和 commentary，请求体没有 `previous_response_id`。
- 第二次的工具调用执行了一次。
- commentary 不在 rollout 历史里。
- 功能关闭时，同一个 516 夹具仍在第一次 `OutputItemDone` 执行工具，且不发第二次模型请求。

相邻回归：普通无指纹响应仍立即执行工具；工具返回后的下一跳仍带上一次干净响应的 `previous_response_id`；取消发生在 `Completed` 前不产生重放请求。

## 10. 实施顺序

1. 写 `reasoning_truncation.rs` 和它的单元测试，并在 `lib.rs` 声明模块。
2. 增加 Feature，处理穷尽匹配。
3. 增加 `discard_websocket_continuation` 和 `client_tests.rs` 测试。
4. 改 `run_sampling_request` 与 `try_run_sampling_request` 的返回值和缓冲区。
5. 本机 `cargo check -p codex-core` 与 `cargo check -p codex-features`，删除 `codex-rs/target`。
6. 提交。未经用户另行授权，不执行 `git push`。

## 11. 明确不做

- 不实现 CodexCont 的 `tool_pair`、`repair_followup`、`forward_marker`。
- 不在重放请求里保留被截断轮的正文或工具调用。
- 不把 commentary 写入用户可见历史来“修复”下一回合。下一回合靠干净响应的 `previous_response_id`；连接断开后的全量重发只包含真实历史。
- 不因为重放改 `prompt_cache_key`，不主动断开 WebSocket。
- 不改上下文压缩的触发和摘要格式。
