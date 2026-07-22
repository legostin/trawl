//! MCP server: config, state, lifecycle.

pub mod core_tools;
pub mod plugin_bridge;
pub mod server;

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use rand::RngCore;
use rmcp::service::Peer;
use rmcp::RoleServer;
use serde::{Deserialize, Serialize};

pub const DEFAULT_PORT: u16 = 9910;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpConfig {
    pub enabled: bool,
    pub port: u16,
    pub token: String,
}

impl Default for McpConfig {
    fn default() -> Self {
        McpConfig { enabled: true, port: DEFAULT_PORT, token: String::new() }
    }
}

pub fn gen_token() -> String {
    let mut b = [0u8; 32];
    rand::rng().fill_bytes(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Короткий id для сущностей, созданных через MCP (у UI — crypto.randomUUID).
pub fn gen_id() -> String {
    let mut b = [0u8; 8];
    rand::rng().fill_bytes(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Читает mcp.json; при отсутствии файла или пустом токене — генерирует токен
/// и сразу сохраняет, чтобы он был стабилен между запусками.
pub fn load_config(dir: &Path) -> McpConfig {
    let mut cfg: McpConfig = fs::read_to_string(dir.join("mcp.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    if cfg.token.is_empty() {
        cfg.token = gen_token();
        let _ = save_config(dir, &cfg);
    }
    cfg
}

pub fn save_config(dir: &Path, cfg: &McpConfig) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    fs::write(dir.join("mcp.json"), text).map_err(|e| e.to_string())
}

/// Подключённые MCP-клиенты — для notifications/tools/list_changed.
pub struct PeerRegistry {
    peers: Mutex<HashMap<u64, Peer<RoleServer>>>,
    counter: AtomicU64,
}

impl PeerRegistry {
    pub fn new() -> Self {
        PeerRegistry { peers: Mutex::new(HashMap::new()), counter: AtomicU64::new(0) }
    }

    pub fn add(&self, peer: Peer<RoleServer>) {
        let id = self.counter.fetch_add(1, Ordering::SeqCst);
        self.peers.lock().unwrap().insert(id, peer);
    }

    /// Шлёт tools/list_changed всем живым пирам; мёртвые выбрасывает.
    /// Пустой реестр — no-op (важно для тестов без async-runtime).
    pub fn notify_tools_changed(self: &Arc<Self>) {
        let snapshot: Vec<(u64, Peer<RoleServer>)> = self
            .peers
            .lock()
            .unwrap()
            .iter()
            .map(|(k, v)| (*k, v.clone()))
            .collect();
        if snapshot.is_empty() {
            return;
        }
        let reg = self.clone();
        tauri::async_runtime::spawn(async move {
            for (id, peer) in snapshot {
                if peer.notify_tool_list_changed().await.is_err() {
                    reg.peers.lock().unwrap().remove(&id);
                }
            }
        });
    }
}

pub struct McpState {
    pub bridge: Arc<plugin_bridge::PluginBridge>,
    pub peers: Arc<PeerRegistry>,
    pub last_error: Mutex<Option<String>>,
    pub server: Mutex<Option<server::ServerHandle>>,
}

impl McpState {
    pub fn new() -> Self {
        McpState {
            bridge: Arc::new(plugin_bridge::PluginBridge::new()),
            peers: Arc::new(PeerRegistry::new()),
            last_error: Mutex::new(None),
            server: Mutex::new(None),
        }
    }
}

/// Останавливает и (если enabled) заново поднимает сервер по конфигу.
pub async fn apply_config(app: &tauri::AppHandle, cfg: &McpConfig) {
    use tauri::Manager;
    let mcp = app.state::<McpState>();
    if let Some(h) = mcp.server.lock().unwrap().take() {
        h.stop();
    }
    *mcp.last_error.lock().unwrap() = None;
    if !cfg.enabled {
        return;
    }
    match server::start_server(app.clone(), cfg.clone()).await {
        Ok(h) => *mcp.server.lock().unwrap() = Some(h),
        Err(e) => *mcp.last_error.lock().unwrap() = Some(e),
    }
}

#[tauri::command]
pub fn mcp_get_config(app: tauri::AppHandle) -> Result<McpConfig, String> {
    Ok(load_config(&crate::commands::data_dir(&app)?))
}

#[tauri::command]
pub async fn mcp_set_config(app: tauri::AppHandle, enabled: bool, port: u16) -> Result<McpConfig, String> {
    let dir = crate::commands::data_dir(&app)?;
    let mut cfg = load_config(&dir);
    cfg.enabled = enabled;
    cfg.port = port;
    save_config(&dir, &cfg)?;
    apply_config(&app, &cfg).await;
    Ok(cfg)
}

#[tauri::command]
pub async fn mcp_regen_token(app: tauri::AppHandle) -> Result<McpConfig, String> {
    let dir = crate::commands::data_dir(&app)?;
    let mut cfg = load_config(&dir);
    cfg.token = gen_token();
    save_config(&dir, &cfg)?;
    apply_config(&app, &cfg).await;
    Ok(cfg)
}

#[tauri::command]
pub fn mcp_server_status(state: tauri::State<'_, McpState>) -> serde_json::Value {
    let server = state.server.lock().unwrap();
    serde_json::json!({
        "running": server.is_some(),
        "addr": server.as_ref().map(|h| h.addr.to_string()),
        "error": *state.last_error.lock().unwrap(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_and_generates_token_once() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = load_config(dir.path());
        assert!(cfg.enabled);
        assert_eq!(cfg.port, DEFAULT_PORT);
        assert_eq!(cfg.token.len(), 64);
        // повторная загрузка возвращает тот же токен (сохранился на диск)
        let again = load_config(dir.path());
        assert_eq!(again.token, cfg.token);
    }

    #[test]
    fn config_roundtrips_camel_case() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = McpConfig { enabled: false, port: 1234, token: "t".into() };
        save_config(dir.path(), &cfg).unwrap();
        let text = std::fs::read_to_string(dir.path().join("mcp.json")).unwrap();
        assert!(text.contains("\"enabled\": false"), "json was: {text}");
        let back = load_config(dir.path());
        assert!(!back.enabled);
        assert_eq!(back.port, 1234);
        assert_eq!(back.token, "t");
    }

    #[test]
    fn gen_id_is_16_hex_chars() {
        let id = gen_id();
        assert_eq!(id.len(), 16);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
