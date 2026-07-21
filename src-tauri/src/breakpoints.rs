use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Breakpoint {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    /// glob over `host+path`, e.g. `api.example.com/*`, `*/login`.
    pub pattern: String,
    /// Optional method filter; None or "*" = any method.
    #[serde(default)]
    pub method: Option<String>,
    pub on_request: bool,
    pub on_response: bool,
    /// Owning project. None = global.
    #[serde(default)]
    pub project_id: Option<String>,
}

impl Breakpoint {
    pub fn matches_target(&self, target: &str) -> bool {
        match crate::rules::glob_to_regex(&self.pattern) {
            Ok(re) => re.is_match(target),
            Err(_) => false,
        }
    }
}

pub fn load_breakpoints(dir: &Path) -> Result<Vec<Breakpoint>> {
    let path = dir.join("breakpoints.json");
    if !path.exists() {
        return Ok(vec![]);
    }
    let text = fs::read_to_string(&path).context("read breakpoints.json")?;
    let bps = serde_json::from_str(&text).context("parse breakpoints.json")?;
    Ok(bps)
}

/// Global breakpoint behaviour (not tied to a single breakpoint).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BreakpointSettings {
    /// Auto-continue a paused flow after N seconds; 0 = hold forever.
    #[serde(default)]
    pub timeout_secs: u64,
    /// While any flow is paused, hold new incoming requests too.
    #[serde(default)]
    pub pause_others: bool,
}

pub fn load_settings(dir: &Path) -> BreakpointSettings {
    let path = dir.join("breakpoint-settings.json");
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_settings(dir: &Path, settings: &BreakpointSettings) -> Result<()> {
    fs::create_dir_all(dir).context("create settings dir")?;
    let text = serde_json::to_string_pretty(settings).context("serialize settings")?;
    fs::write(dir.join("breakpoint-settings.json"), text).context("write settings")?;
    Ok(())
}

pub fn save_breakpoints(dir: &Path, bps: &[Breakpoint]) -> Result<()> {
    fs::create_dir_all(dir).context("create breakpoints dir")?;
    let text = serde_json::to_string_pretty(bps).context("serialize breakpoints")?;
    fs::write(dir.join("breakpoints.json"), text).context("write breakpoints.json")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bp(pattern: &str) -> Breakpoint {
        Breakpoint {
            id: "1".into(),
            name: "t".into(),
            enabled: true,
            pattern: pattern.into(),
            method: None,
            on_request: true,
            on_response: false,
            project_id: None,
        }
    }

    #[test]
    fn matches_host_path_glob() {
        let b = bp("api.example.com/*");
        assert!(b.matches_target("api.example.com/v1/users"));
        assert!(!b.matches_target("cdn.example.com/v1/users"));
    }

    #[test]
    fn breakpoints_roundtrip_to_disk() {
        let tmp = std::env::temp_dir().join(format!("trawl-bp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(load_breakpoints(&tmp).unwrap().is_empty());
        save_breakpoints(&tmp, &[bp("api.example.com/*")]).unwrap();
        let back = load_breakpoints(&tmp).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].pattern, "api.example.com/*");
        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
