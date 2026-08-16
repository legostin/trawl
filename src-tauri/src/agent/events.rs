use serde::Serialize;

/// Token counts we show in the UI. Cache fields are deliberately not surfaced:
/// they confuse more than they inform.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// What the UI is told, whichever harness produced it. Adding a harness must
/// not add a variant here; if it wants to, the abstraction is wrong.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AgentEvent {
    SessionStarted {
        session_id: String,
        model: String,
        /// Whether our own MCP server reported itself connected.
        trawl_connected: bool,
    },
    AssistantText {
        text: String,
    },
    Reasoning {
        text: String,
    },
    ToolCall {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    TurnDone {
        text: String,
        usage: Usage,
    },
    Error {
        message: String,
    },
}
