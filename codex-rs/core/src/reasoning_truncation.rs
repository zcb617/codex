//! 判断本轮 reasoning 是否落在 516 截断指纹上，并构造不入库的续想 commentary。

use codex_protocol::models::ContentItem;
use codex_protocol::models::MessagePhase;
use codex_protocol::models::ResponseItem;

/// 相邻截断档位的 token 间距。指纹为 518*n-2。
const TRUNCATION_STEP: i64 = 518;
/// 允许重放的最小档位。516 对应 n=1。
const MIN_N: i64 = 1;
/// 允许重放的最大档位。3106 对应 n=6，3624 对应 n=7 不再重放。
const MAX_N: i64 = 6;
/// 同一采样内最多重放的轮次。第 4 轮起接受当前结果。
const MAX_CONTINUE: u32 = 3;
/// 重放请求末尾追加的 commentary 文本，不写入会话历史。
const MARKER_TEXT: &str = "Continue thinking...";

/// 本轮 reasoning token 用量的处理决定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TruncationDecision {
    /// 丢弃本轮正文和工具调用，不接被截断响应，整轮重放。
    Replay,
    /// 交出本轮已产生的正文和工具调用。
    Accept {
        /// 命中截断指纹但因缺加密内容、轮次或档位而不能重放。
        still_truncated: bool,
    },
}

/// 按固定顺序判断本轮是否重放：先看指纹，再看加密内容、轮次和档位。
pub(crate) fn decide(
    reasoning_output_tokens: Option<i64>,
    round_no: u32,
    last_reasoning_has_encrypted_content: bool,
) -> TruncationDecision {
    let Some(tokens) = reasoning_output_tokens else {
        return TruncationDecision::Accept {
            still_truncated: false,
        };
    };
    if tokens < TRUNCATION_STEP - 2 || (tokens + 2) % TRUNCATION_STEP != 0 {
        return TruncationDecision::Accept {
            still_truncated: false,
        };
    }
    if !last_reasoning_has_encrypted_content {
        return TruncationDecision::Accept {
            still_truncated: true,
        };
    }
    if round_no > MAX_CONTINUE {
        return TruncationDecision::Accept {
            still_truncated: true,
        };
    }
    let tier_n = (tokens + 2) / TRUNCATION_STEP;
    if tier_n < MIN_N || tier_n > MAX_N {
        return TruncationDecision::Accept {
            still_truncated: true,
        };
    }
    TruncationDecision::Replay
}

/// 构造只附在当次重放请求上的 assistant commentary，调用方不得把它写入历史。
pub(crate) fn commentary_marker() -> ResponseItem {
    ResponseItem::Message {
        encrypted_content: None,
        status: None,
        id: None,
        role: "assistant".to_string(),
        content: vec![ContentItem::OutputText {
            annotations: None,
            logprobs: None,
            text: MARKER_TEXT.to_string(),
        }],
        phase: Some(MessagePhase::Commentary),
        internal_chat_message_metadata_passthrough: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 已知 1..=6 档在首轮且有加密内容时都要重放。
    #[test]
    fn truncation_pattern_replays_known_tiers() {
        for tokens in [516_i64, 1034, 1552, 2070, 2588, 3106] {
            assert_eq!(
                decide(Some(tokens), 1, true),
                TruncationDecision::Replay,
                "{tokens}",
            );
        }
    }

    /// 缺失用量和非指纹用量直接接受，且不标记为仍被截断。
    #[test]
    fn non_pattern_is_accepted() {
        for tokens in [None, Some(0), Some(515), Some(517), Some(518)] {
            assert_eq!(
                decide(tokens, 1, true),
                TruncationDecision::Accept {
                    still_truncated: false,
                },
                "{tokens:?}",
            );
        }
    }

    /// 第 4 轮即使仍是 516 指纹，也接受并标记为仍被截断。
    #[test]
    fn fourth_round_of_516_accepts_still_truncated() {
        assert_eq!(
            decide(Some(516), 4, true),
            TruncationDecision::Accept {
                still_truncated: true,
            },
        );
    }

    /// 超过第 6 档的指纹不再重放，接受并标记为仍被截断。
    #[test]
    fn tier_above_max_accepts() {
        assert_eq!(
            decide(Some(3624), 1, true),
            TruncationDecision::Accept {
                still_truncated: true,
            },
        );
    }

    /// 命中指纹但最后一条 reasoning 没有非空加密内容时，接受并标记为仍被截断。
    #[test]
    fn missing_encrypted_content_accepts() {
        assert_eq!(
            decide(Some(516), 1, false),
            TruncationDecision::Accept {
                still_truncated: true,
            },
        );
    }

    /// 重放 commentary 必须是无 id 的 assistant commentary，文本固定。
    #[test]
    fn commentary_marker_shape() {
        let marker = commentary_marker();
        assert!(matches!(
            marker,
            ResponseItem::Message {
                id: None,
                role,
                phase: Some(MessagePhase::Commentary),
                content,
                ..
            } if role == "assistant"
                && matches!(
                    content.as_slice(),
                    [ContentItem::OutputText { text, .. }] if text == "Continue thinking..."
                )
        ));
    }
}
