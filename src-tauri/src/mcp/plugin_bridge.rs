//! Мост плагинных MCP-тулов: реестр метаданных (Rust) + вызовы JS-handler-ов
//! через Tauri-событие `mcp:tool-call` и команду-ответ `mcp_tool_result`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, RwLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::State;
use tokio::sync::oneshot;

pub const DEFAULT_TIMEOUT_MS: u64 = 60_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginTool {
    pub plugin_id: String,
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

impl PluginTool {
    pub fn full_name(&self) -> String {
        format!("{}_{}", self.plugin_id, self.name)
    }
}

#[derive(Default)]
pub struct PluginBridge {
    pub tools: RwLock<Vec<PluginTool>>,
    pending: Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>,
    counter: AtomicU64,
}

impl PluginBridge {
    pub fn new() -> Self {
        Self::default()
    }

    /// Регистрирует тул; повторная регистрация того же plugin_id+name замещает.
    pub fn register(&self, tool: PluginTool) {
        let mut tools = self.tools.write().unwrap();
        tools.retain(|t| !(t.plugin_id == tool.plugin_id && t.name == tool.name));
        tools.push(tool);
    }

    pub fn unregister(&self, plugin_id: &str, name: &str) {
        self.tools
            .write()
            .unwrap()
            .retain(|t| !(t.plugin_id == plugin_id && t.name == name));
    }

    pub fn clear_plugin(&self, plugin_id: &str) {
        self.tools.write().unwrap().retain(|t| t.plugin_id != plugin_id);
    }

    pub fn find(&self, full_name: &str) -> Option<PluginTool> {
        self.tools.read().unwrap().iter().find(|t| t.full_name() == full_name).cloned()
    }

    /// Вызов плагинного тула: `emit` доставляет payload в webview, ответ
    /// приходит через `resolve` (команда mcp_tool_result). Таймаут — ошибка.
    pub async fn call(
        &self,
        emit: impl Fn(Value),
        tool: &PluginTool,
        args: Value,
    ) -> Result<Value, String> {
        let call_id = self.counter.fetch_add(1, Ordering::SeqCst) + 1;
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(call_id, tx);
        emit(json!({ "callId": call_id, "tool": tool.full_name(), "args": args }));
        let timeout = Duration::from_millis(tool.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS));
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(res)) => res,
            Ok(Err(_)) => Err(format!("plugin “{}” dropped the call", tool.plugin_id)),
            Err(_) => {
                self.pending.lock().unwrap().remove(&call_id);
                Err(format!("plugin tool “{}” timed out", tool.full_name()))
            }
        }
    }

    pub fn resolve(&self, call_id: u64, result: Result<Value, String>) {
        if let Some(tx) = self.pending.lock().unwrap().remove(&call_id) {
            let _ = tx.send(result);
        }
    }
}

// ── Tauri-команды (вызываются фронтовым мостом) ──

#[tauri::command]
pub fn mcp_register_tool(
    plugin_id: String,
    name: String,
    description: String,
    input_schema: Value,
    timeout_ms: Option<u64>,
    state: State<'_, super::McpState>,
) -> Result<(), String> {
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err("tool name must be [a-zA-Z0-9_-]".into());
    }
    state.bridge.register(PluginTool { plugin_id, name, description, input_schema, timeout_ms });
    state.peers.notify_tools_changed();
    Ok(())
}

#[tauri::command]
pub fn mcp_unregister_tool(plugin_id: String, name: String, state: State<'_, super::McpState>) {
    state.bridge.unregister(&plugin_id, &name);
    state.peers.notify_tools_changed();
}

#[tauri::command]
pub fn mcp_clear_plugin_tools(plugin_id: String, state: State<'_, super::McpState>) {
    state.bridge.clear_plugin(&plugin_id);
    state.peers.notify_tools_changed();
}

#[tauri::command]
pub fn mcp_tool_result(
    call_id: u64,
    result: Option<Value>,
    error: Option<String>,
    state: State<'_, super::McpState>,
) {
    let res = match error {
        Some(e) => Err(e),
        None => Ok(result.unwrap_or(Value::Null)),
    };
    state.bridge.resolve(call_id, res);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tool(plugin: &str, name: &str) -> PluginTool {
        PluginTool {
            plugin_id: plugin.into(),
            name: name.into(),
            description: "d".into(),
            input_schema: json!({ "type": "object" }),
            timeout_ms: None,
        }
    }

    #[test]
    fn register_replaces_and_clear_removes() {
        let b = PluginBridge::new();
        b.register(tool("p1", "t"));
        b.register(tool("p1", "t")); // повторная регистрация замещает
        b.register(tool("p2", "t"));
        assert_eq!(b.tools.read().unwrap().len(), 2);
        assert!(b.find("p1_t").is_some());
        b.clear_plugin("p1");
        assert!(b.find("p1_t").is_none());
        assert!(b.find("p2_t").is_some());
        b.unregister("p2", "t");
        assert!(b.tools.read().unwrap().is_empty());
    }

    #[tokio::test]
    async fn call_resolves_with_result_from_webview() {
        let b = std::sync::Arc::new(PluginBridge::new());
        let t = tool("p", "echo");
        b.register(t.clone());
        let b2 = b.clone();
        let fut = b.call(
            move |payload| {
                // имитируем webview: сразу отвечаем на пришедший callId
                let call_id = payload["callId"].as_u64().unwrap();
                b2.resolve(call_id, Ok(json!({ "echo": payload["args"] })));
            },
            &t,
            json!({ "x": 1 }),
        );
        let out = fut.await.unwrap();
        assert_eq!(out["echo"]["x"], json!(1));
    }

    #[tokio::test]
    async fn call_times_out() {
        let b = PluginBridge::new();
        let mut t = tool("p", "slow");
        t.timeout_ms = Some(50);
        b.register(t.clone());
        let err = b.call(|_| {}, &t, json!({})).await.unwrap_err();
        assert!(err.contains("timed out"), "err was: {err}");
    }
}
