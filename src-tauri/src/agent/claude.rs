use crate::agent::events::{AgentEvent, Usage};
use serde_json::Value;

/// One line of `claude --output-format stream-json` becomes zero or more UI
/// events. Unknown event types yield nothing: the harness emits hook,
/// token-estimate and rate-limit chatter that the chat has no use for, and new
/// kinds appear across versions.
pub fn parse_line(line: &str) -> Vec<AgentEvent> {
    let line = line.trim();
    if line.is_empty() {
        return vec![];
    }
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return vec![AgentEvent::Error {
            message: format!("unreadable line from claude: {}", truncate(line, 200)),
        }];
    };
    match v.get("type").and_then(Value::as_str) {
        Some("system") if v.get("subtype").and_then(Value::as_str) == Some("init") => {
            vec![AgentEvent::SessionStarted {
                session_id: str_at(&v, "session_id"),
                model: str_at(&v, "model"),
                trawl_connected: v
                    .get("mcp_servers")
                    .and_then(Value::as_array)
                    .is_some_and(|servers| {
                        servers.iter().any(|s| {
                            s.get("name").and_then(Value::as_str) == Some("trawl")
                                && s.get("status").and_then(Value::as_str) == Some("connected")
                        })
                    }),
            }]
        }
        Some("assistant") => v
            .pointer("/message/content")
            .and_then(Value::as_array)
            .map(|blocks| blocks.iter().filter_map(block_event).collect())
            .unwrap_or_default(),
        Some("result") => {
            let text = str_at(&v, "result");
            if v.get("is_error").and_then(Value::as_bool) == Some(true) {
                vec![AgentEvent::Error { message: text }]
            } else {
                vec![AgentEvent::TurnDone {
                    text,
                    usage: usage_at(&v),
                }]
            }
        }
        _ => vec![],
    }
}

fn block_event(block: &Value) -> Option<AgentEvent> {
    match block.get("type").and_then(Value::as_str) {
        Some("text") => {
            let text = str_at(block, "text");
            (!text.is_empty()).then_some(AgentEvent::AssistantText { text })
        }
        Some("thinking") => {
            // Empty thinking blocks are a normal artefact of redacted reasoning.
            let text = str_at(block, "thinking");
            (!text.is_empty()).then_some(AgentEvent::Reasoning { text })
        }
        Some("tool_use") => Some(AgentEvent::ToolCall {
            id: str_at(block, "id"),
            name: str_at(block, "name"),
            input: block.get("input").cloned().unwrap_or(Value::Null),
        }),
        _ => None,
    }
}

fn usage_at(v: &Value) -> Usage {
    Usage {
        input_tokens: u64_at(v, "/usage/input_tokens"),
        output_tokens: u64_at(v, "/usage/output_tokens"),
    }
}

fn str_at(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn u64_at(v: &Value, ptr: &str) -> u64 {
    v.pointer(ptr).and_then(Value::as_u64).unwrap_or(0)
}

fn truncate(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELLO: &str = include_str!("fixtures/claude-hello.jsonl");

    fn parse_all(s: &str) -> Vec<AgentEvent> {
        s.lines().flat_map(parse_line).collect()
    }

    #[test]
    fn init_announces_the_session_and_our_mcp_server() {
        let events = parse_all(HELLO);
        assert_eq!(
            events[0],
            AgentEvent::SessionStarted {
                session_id: "2f0f77e2-f2a3-4323-b897-5833ab904c62".into(),
                model: "claude-opus-5".into(),
                trawl_connected: true,
            }
        );
    }

    #[test]
    fn an_assistant_message_yields_reasoning_then_text() {
        let events = parse_all(HELLO);
        assert!(events.contains(&AgentEvent::Reasoning {
            text: "The user wants a greeting.".into()
        }));
        assert!(events.contains(&AgentEvent::AssistantText { text: "hi".into() }));
    }

    #[test]
    fn the_result_line_closes_the_turn_with_usage() {
        let events = parse_all(HELLO);
        assert_eq!(
            events.last().unwrap(),
            &AgentEvent::TurnDone {
                text: "hi".into(),
                usage: Usage {
                    input_tokens: 2,
                    output_tokens: 75
                },
            }
        );
    }

    #[test]
    fn hooks_and_token_counters_are_ignored_rather_than_guessed_at() {
        // Three of the five fixture lines carry nothing the UI can show.
        // A harness upgrade adding more of them must not produce noise.
        assert!(parse_line(r#"{"type":"system","subtype":"hook_started"}"#).is_empty());
        assert!(parse_line(r#"{"type":"rate_limit_event","rate_limit_info":{}}"#).is_empty());
        assert!(parse_line("").is_empty());
    }

    #[test]
    fn a_tool_call_carries_its_name_and_input() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"mcp__trawl__query_flows","input":{"status":500}}]}}"#;
        assert_eq!(
            parse_line(line),
            vec![AgentEvent::ToolCall {
                id: "t1".into(),
                name: "mcp__trawl__query_flows".into(),
                input: serde_json::json!({"status": 500}),
            }]
        );
    }

    #[test]
    fn a_failed_turn_is_surfaced_as_an_error() {
        let line = r#"{"type":"result","subtype":"error_during_execution","is_error":true,"result":"rate limited"}"#;
        assert_eq!(
            parse_line(line),
            vec![AgentEvent::Error {
                message: "rate limited".into()
            }]
        );
    }

    #[test]
    fn a_broken_line_is_reported_instead_of_swallowed() {
        // stdout is strictly JSONL. Anything else means the protocol moved
        // under us, and silence would look like the agent simply went quiet.
        let events = parse_line("not json at all");
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], AgentEvent::Error { .. }));
    }
}
