//! Plugin registry + manual GitHub install.
//!
//! A plugin is a GitHub repo containing `trawl-plugin.json` (manifest) and a
//! built JS bundle. Users add a plugin by repo reference; we fetch the manifest
//! and the entry bundle over HTTPS, cache the bundle under
//! `app_data_dir/plugins/<id>/plugin.js`, and record it in `plugins.json`.
//! The frontend loader later reads the cached bundle and executes it.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

/// Manifest fetched from the plugin repo (`trawl-plugin.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    /// Path (within the repo) to the built JS bundle, e.g. "dist/plugin.js".
    pub entry: String,
    #[serde(default)]
    pub api_version: String,
}

/// Installed-plugin record persisted in `plugins.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    /// "owner/repo".
    pub repo: String,
    /// Git host, e.g. "github.com" or "github.example.org".
    #[serde(default = "default_host")]
    pub host: String,
    /// Git ref (tag/branch/commit) the bundle was fetched from.
    #[serde(rename = "ref")]
    pub git_ref: String,
    pub enabled: bool,
}

fn default_host() -> String {
    "github.com".to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginsFile {
    pub plugins: Vec<Plugin>,
}

pub fn plugins_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("plugins")
}

pub fn load_plugins(data_dir: &Path) -> Result<PluginsFile> {
    let path = plugins_dir(data_dir).join("plugins.json");
    if !path.exists() {
        return Ok(PluginsFile::default());
    }
    let text = fs::read_to_string(&path).context("read plugins.json")?;
    serde_json::from_str(&text).context("parse plugins.json")
}

pub fn save_plugins(data_dir: &Path, file: &PluginsFile) -> Result<()> {
    let dir = plugins_dir(data_dir);
    fs::create_dir_all(&dir).context("create plugins dir")?;
    let text = serde_json::to_string_pretty(file)?;
    fs::write(dir.join("plugins.json"), text).context("write plugins.json")?;
    Ok(())
}

/// Normalize a user-entered repo reference into `(host, "owner/repo", "ref")`.
/// Accepts `owner/repo`, `owner/repo@ref`, full URLs (github.com or a GHE host),
/// and `.../tree/<ref>`. A leading segment containing a dot is treated as a host;
/// otherwise the host defaults to github.com. An explicit `reference` argument
/// wins; otherwise `@ref`/`tree/<ref>`; else `main`.
pub fn normalize_repo(input: &str, reference: Option<&str>) -> (String, String, String) {
    let mut s = input.trim();
    for p in ["https://", "http://", "www."] {
        s = s.trim_start_matches(p);
    }
    let (repo_part, ref_at) = match s.split_once('@') {
        Some((r, rf)) => (r, Some(rf.to_string())),
        None => (s, None),
    };
    let cleaned = repo_part.trim_matches('/');
    let mut parts: Vec<&str> = cleaned.split('/').filter(|p| !p.is_empty()).collect();
    let host = if parts.first().is_some_and(|p| p.contains('.')) {
        parts.remove(0).to_string()
    } else {
        "github.com".to_string()
    };
    let mut tree_ref = None;
    let repo = if parts.len() >= 4 && parts[2] == "tree" {
        tree_ref = Some(parts[3].to_string());
        format!("{}/{}", parts[0], parts[1])
    } else if parts.len() >= 2 {
        format!("{}/{}", parts[0], parts[1])
    } else {
        parts.join("/")
    };
    let repo = repo.trim_end_matches(".git").to_string();
    let git_ref = reference
        .map(|s| s.to_string())
        .or(ref_at)
        .or(tree_ref)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "main".to_string());
    (host, repo, git_ref)
}

/// API base for a host: github.com uses api.github.com, GHE uses `/api/v3`.
pub fn api_url(host: &str, repo: &str, path: &str) -> String {
    if host == "github.com" {
        format!("https://api.github.com/repos/{repo}/{path}")
    } else {
        format!("https://{host}/api/v3/repos/{repo}/{path}")
    }
}

// Placeholder encoders until the git-browse task replaces them with real
// percent-encoding (paths in the contracts repo contain spaces and Cyrillic).
fn encode_seg(s: &str) -> String {
    s.to_string()
}
fn encode_path(p: &str) -> String {
    p.to_string()
}

/// GitHub Contents API URL. Unlike raw.githubusercontent.com (Fastly-cached ~5min),
/// the API returns fresh content, so freshly-pushed plugin versions are seen at once.
/// Files must be ≤ 1 MB (plugin bundles are tiny).
fn api_content_url(host: &str, repo: &str, git_ref: &str, file: &str) -> String {
    api_url(
        host,
        repo,
        &format!(
            "contents/{}?ref={}",
            encode_path(file.trim_start_matches('/')),
            encode_seg(git_ref)
        ),
    )
}

fn http_get_text(url: &str) -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("trawl-plugin-installer")
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get(url)
        .header("Accept", "application/vnd.github.raw")
        .send()
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {} for {url}", resp.status()));
    }
    resp.text().map_err(|e| e.to_string())
}

fn fetch_manifest_blocking(host: &str, repo: &str, git_ref: &str) -> Result<PluginManifest, String> {
    let text = http_get_text(&api_content_url(host, repo, git_ref, "trawl-plugin.json"))?;
    serde_json::from_str::<PluginManifest>(&text).map_err(|e| format!("invalid manifest: {e}"))
}

/// Resolve the effective host: one parsed out of the repo input wins; otherwise
/// an explicit `host` argument (stored plugin records pass it); else github.com.
fn effective_host(parsed: String, explicit: Option<String>) -> String {
    if parsed != "github.com" {
        return parsed;
    }
    explicit.filter(|s| !s.trim().is_empty()).unwrap_or(parsed)
}

fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

// ── Tauri commands ──

#[tauri::command]
pub async fn fetch_plugin_manifest(
    repo: String,
    reference: Option<String>,
    host: Option<String>,
) -> Result<PluginManifest, String> {
    let (parsed_host, repo, git_ref) = normalize_repo(&repo, reference.as_deref());
    let host = effective_host(parsed_host, host);
    tokio::task::spawn_blocking(move || fetch_manifest_blocking(&host, &repo, &git_ref))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn install_plugin(
    app: AppHandle,
    repo: String,
    reference: Option<String>,
    host: Option<String>,
) -> Result<Vec<Plugin>, String> {
    let (parsed_host, repo, git_ref) = normalize_repo(&repo, reference.as_deref());
    let host = effective_host(parsed_host, host);
    let data = data_dir(&app)?;

    // Fetch manifest + bundle off the async runtime.
    let host_c = host.clone();
    let repo_c = repo.clone();
    let ref_c = git_ref.clone();
    let (manifest, bundle) = tokio::task::spawn_blocking(move || {
        let m = fetch_manifest_blocking(&host_c, &repo_c, &ref_c)?;
        let code = http_get_text(&api_content_url(&host_c, &repo_c, &ref_c, &m.entry))?;
        Ok::<_, String>((m, code))
    })
    .await
    .map_err(|e| e.to_string())??;

    if manifest.id.trim().is_empty() {
        return Err("manifest is missing an id".into());
    }

    // Cache the bundle.
    let dir = plugins_dir(&data).join(&manifest.id);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    fs::write(dir.join("plugin.js"), &bundle).map_err(|e| e.to_string())?;

    // Upsert the registry entry.
    let mut file = load_plugins(&data).map_err(|e| e.to_string())?;
    let plugin = Plugin {
        id: manifest.id.clone(),
        name: if manifest.name.is_empty() { manifest.id.clone() } else { manifest.name },
        version: manifest.version,
        description: manifest.description,
        author: manifest.author,
        repo,
        host,
        git_ref,
        enabled: true,
    };
    if let Some(e) = file.plugins.iter_mut().find(|p| p.id == plugin.id) {
        *e = plugin;
    } else {
        file.plugins.push(plugin);
    }
    save_plugins(&data, &file).map_err(|e| e.to_string())?;
    Ok(file.plugins)
}

#[tauri::command]
pub fn list_plugins(app: AppHandle) -> Result<Vec<Plugin>, String> {
    Ok(load_plugins(&data_dir(&app)?).map_err(|e| e.to_string())?.plugins)
}

#[tauri::command]
pub fn set_plugin_enabled(
    app: AppHandle,
    id: String,
    enabled: bool,
) -> Result<Vec<Plugin>, String> {
    let data = data_dir(&app)?;
    let mut file = load_plugins(&data).map_err(|e| e.to_string())?;
    if let Some(p) = file.plugins.iter_mut().find(|p| p.id == id) {
        p.enabled = enabled;
    }
    save_plugins(&data, &file).map_err(|e| e.to_string())?;
    Ok(file.plugins)
}

#[tauri::command]
pub fn remove_plugin(app: AppHandle, id: String) -> Result<Vec<Plugin>, String> {
    let data = data_dir(&app)?;
    let mut file = load_plugins(&data).map_err(|e| e.to_string())?;
    file.plugins.retain(|p| p.id != id);
    let _ = fs::remove_dir_all(plugins_dir(&data).join(&id));
    save_plugins(&data, &file).map_err(|e| e.to_string())?;
    Ok(file.plugins)
}

#[tauri::command]
pub fn read_plugin_bundle(app: AppHandle, id: String) -> Result<String, String> {
    let path = plugins_dir(&data_dir(&app)?).join(&id).join("plugin.js");
    fs::read_to_string(&path).map_err(|e| e.to_string())
}

// ── Plugin key/value storage (JSON blobs) ──

/// Sanitize a storage key into a safe single filename component.
fn safe_key(key: &str) -> String {
    let cleaned: String = key
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '_' })
        .collect();
    let trimmed = cleaned.trim_matches('.');
    if trimmed.is_empty() { "_".to_string() } else { trimmed.to_string() }
}

fn plugin_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(data_dir(app)?.join("plugin-data"))
}

#[tauri::command]
pub fn plugin_storage_get(app: AppHandle, key: String) -> Result<Option<String>, String> {
    let path = plugin_data_dir(&app)?.join(format!("{}.json", safe_key(&key)));
    if !path.exists() {
        return Ok(None);
    }
    fs::read_to_string(&path).map(Some).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn plugin_storage_set(app: AppHandle, key: String, value: String) -> Result<(), String> {
    let dir = plugin_data_dir(&app)?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    fs::write(dir.join(format!("{}.json", safe_key(&key))), value).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_plain_repo_defaults_to_main() {
        assert_eq!(
            normalize_repo("owner/repo", None),
            ("github.com".into(), "owner/repo".into(), "main".into())
        );
    }

    #[test]
    fn normalize_at_ref() {
        assert_eq!(
            normalize_repo("owner/repo@v1.2.3", None),
            ("github.com".into(), "owner/repo".into(), "v1.2.3".into())
        );
    }

    #[test]
    fn normalize_full_url_and_tree_ref() {
        assert_eq!(
            normalize_repo("https://github.com/owner/repo", None),
            ("github.com".into(), "owner/repo".into(), "main".into())
        );
        assert_eq!(
            normalize_repo("https://github.com/owner/repo/tree/dev", None),
            ("github.com".into(), "owner/repo".into(), "dev".into())
        );
    }

    #[test]
    fn explicit_reference_wins_and_git_suffix_stripped() {
        assert_eq!(
            normalize_repo("owner/repo.git@v1", Some("main")),
            ("github.com".into(), "owner/repo".into(), "main".into())
        );
    }

    #[test]
    fn normalize_extracts_enterprise_host() {
        assert_eq!(
            normalize_repo("github.example.org/acme/trawl-plugin-contracts", None),
            (
                "github.example.org".into(),
                "acme/trawl-plugin-contracts".into(),
                "main".into()
            )
        );
        assert_eq!(
            normalize_repo("https://github.example.org/acme/dev-contracts/tree/KL-30089", None),
            ("github.example.org".into(), "acme/dev-contracts".into(), "KL-30089".into())
        );
    }

    #[test]
    fn api_url_per_host() {
        assert_eq!(
            api_url("github.com", "o/r", "contents/trawl-plugin.json"),
            "https://api.github.com/repos/o/r/contents/trawl-plugin.json"
        );
        assert_eq!(
            api_url("github.example.org", "o/r", "branches"),
            "https://github.example.org/api/v3/repos/o/r/branches"
        );
    }

    #[test]
    fn safe_key_sanitizes() {
        assert_eq!(safe_key("collections.proj-1"), "collections.proj-1");
        assert_eq!(safe_key("a/b:c"), "a_b_c");
        assert_eq!(safe_key(""), "_");
        // No path traversal survives: slashes are neutralized to a single component.
        let s = safe_key("../../etc/passwd");
        assert!(!s.contains('/'));
        assert!(!s.contains("..") || !s.contains('/'));
    }

    #[test]
    fn registry_roundtrip() {
        let dir = std::env::temp_dir().join(format!("trawl-plugins-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        assert!(load_plugins(&dir).unwrap().plugins.is_empty());
        let file = PluginsFile {
            plugins: vec![Plugin {
                id: "a".into(),
                name: "A".into(),
                version: "1.0.0".into(),
                description: String::new(),
                author: String::new(),
                repo: "o/r".into(),
                host: "github.com".into(),
                git_ref: "main".into(),
                enabled: true,
            }],
        };
        save_plugins(&dir, &file).unwrap();
        let back = load_plugins(&dir).unwrap();
        assert_eq!(back.plugins.len(), 1);
        assert_eq!(back.plugins[0].id, "a");
        let _ = fs::remove_dir_all(&dir);
    }
}
