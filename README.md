# Codex CLI 定制版

本项目基于 [OpenAI Codex](https://github.com/openai/codex) 维护，重点改进长会话、Skills 和多代理协作，并提供 Windows、Linux、macOS 命令行发行包。

## 主要改动

### 父子代理服务等级独立

父代理和子代理分别使用自己的 `service_tier`。父代理开启或关闭 Fast，不会改变子代理的服务等级；子代理按自己的角色配置运行，未配置时使用 `default`。这一规则也适用于子代理继续执行、恢复会话和 WebSocket 预连接。

例如，在已有的 `~/.codex/agents/finder.toml` 中设置：

```toml
service_tier = "default"
```

父代理开启 Fast 后，finder 仍按 `default` 运行。也可以将子代理配置为 `priority`，在模型和功能开关支持时单独使用 Fast。

`default` 会使 Codex 在请求中省略 `service_tier` 字段；实际服务档位还可能受服务端默认设置影响。

### 压缩后立即注入上下文

上下文压缩成功后，`PostCompact` hook 可以通过 `additionalContext` 补充项目约定、任务要求等内容。内容会在压缩后立即加入会话，同一轮任务继续执行时就能使用。

Hook 的标准输出示例：

```json
{
  "hookSpecificOutput": {
    "hookEventName": "PostCompact",
    "additionalContext": "继续遵守项目约定，完成当前任务。"
  }
}
```

配置方法和完整示例见 [PostCompact 使用说明](codex-rs/hooks/POST_COMPACT_ADDITIONAL_CONTEXT.md)。

### 提高 Skills 目录预算

提高提供给模型的 Skills 名称、描述等目录信息的默认预算，减少技能较多时因预算不足而被省略的情况：

- 已知模型上下文窗口时，默认预算从窗口的 **2% 提高到 4%**。
- 未知模型上下文窗口时，默认字符预算从 **8,000 提高到 16,000**。

如果显式设置了 `skills.max_context_tokens`，则使用该配置值并遵循现有上限。

### 后台服务默认关闭

`daemon_auto_start` 默认关闭，直接启动 `codex` 时不会自动进入后台服务启动流程。如需使用，需具备后台服务所需的完整安装包，并在 `~/.codex/config.toml` 中显式启用：

```toml
[features]
daemon_auto_start = true
```

## 下载与运行

从本项目的 [GitHub Releases](https://github.com/zcb617/codex/releases) 下载对应平台的压缩包：

| 平台 | 架构 |
| --- | --- |
| Windows | x86_64 |
| Linux | x86_64（GNU） |
| macOS | Apple Silicon、x86_64 |

解压后将包内文件保留在同一目录。进入该目录后，在 Linux 或 macOS 下运行 `./codex`，在 Windows PowerShell 中运行 `.\codex.exe`。发行包包含的功能以对应的发布版本为准；尚未发布的修改以开发分支为准。

## 许可证

本项目采用 [Apache-2.0 许可证](LICENSE)。
