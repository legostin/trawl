//! MCP server: config, state, lifecycle.

use std::fs;
use std::path::Path;

use rand::RngCore;
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
