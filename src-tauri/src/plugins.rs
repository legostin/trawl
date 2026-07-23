//! Plugin registry + manual GitHub install.
//!
//! A plugin is a GitHub repo containing `trawl-plugin.json` (manifest) and a
//! built JS bundle. Users add a plugin by repo reference; we fetch the manifest
//! and the entry bundle over HTTPS, cache the bundle under
//! `app_data_dir/plugins/<id>/plugin.js`, and record it in `plugins.json`.
//! The frontend loader later reads the cached bundle and executes it.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

/// A plugin this plugin needs installed (from `trawl-plugin.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginDep {
    pub id: String,
    /// "owner/repo" to install the dependency from.
    pub repo: String,
    #[serde(default = "default_host")]
    pub host: String,
    /// Git ref; defaults to "main".
    #[serde(default)]
    pub reference: Option<String>,
    /// Reinstall the dependency when the installed version is older.
    #[serde(default)]
    pub min_version: Option<String>,
}

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
    #[serde(default)]
    pub dependencies: Vec<PluginDep>,
}

/// Dotted-numeric version compare ("0.10.1" > "0.9.9").
fn cmp_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |s: &str| -> Vec<u64> {
        s.split('.').map(|x| x.trim().parse::<u64>().unwrap_or(0)).collect()
    };
    let (va, vb) = (parse(a), parse(b));
    for i in 0..va.len().max(vb.len()) {
        let d = va.get(i).unwrap_or(&0).cmp(vb.get(i).unwrap_or(&0));
        if d != std::cmp::Ordering::Equal {
            return d;
        }
    }
    std::cmp::Ordering::Equal
}

/// Refuse a manifest that needs a newer host↔plugin API than this app provides.
/// An empty `manifest_api` (pre-gate manifests) or a missing `host_api` (older
/// frontend that doesn't pass it) skips the check.
fn check_api_version(name: &str, manifest_api: &str, host_api: Option<&str>) -> Result<(), String> {
    let Some(host_api) = host_api else { return Ok(()) };
    if manifest_api.trim().is_empty()
        || cmp_versions(manifest_api, host_api) != std::cmp::Ordering::Greater
    {
        return Ok(());
    }
    Err(format!(
        "\"{name}\" requires plugin API {manifest_api}, but this app provides {host_api} — update the app first"
    ))
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
    /// Plugin API version the bundle needs (manifest `apiVersion`; "" for
    /// plugins installed before this was recorded).
    #[serde(default)]
    pub api_version: String,
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

/// Everything beyond unreserved ASCII is escaped — repo paths in contract repos
/// contain spaces and Cyrillic, and branch names may contain slashes.
const SEG: &percent_encoding::AsciiSet = &percent_encoding::CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}')
    .add(b'/')
    .add(b'\\')
    .add(b'^')
    .add(b'|')
    .add(b'[')
    .add(b']')
    .add(b'@')
    .add(b':')
    .add(b'&')
    .add(b'+')
    .add(b'=');

fn encode_seg(s: &str) -> String {
    percent_encoding::utf8_percent_encode(s, SEG).to_string()
}

/// Encode each path segment, keeping `/` separators.
fn encode_path(p: &str) -> String {
    p.split('/').map(encode_seg).collect::<Vec<_>>().join("/")
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

/// URL to fetch one file's raw content from, picked by auth state.
///
/// Authenticated fetches use the Contents API (fresh, no CDN cache). Without a
/// token, github.com goes through raw.githubusercontent.com instead: anonymous
/// api.github.com calls share a 60 req/h per-IP quota and 403 once it's spent
/// (guaranteed on office NAT/VPN), while the raw host is uncapped — at the
/// cost of Fastly's ~5 min cache. GHE keeps the API either way.
fn content_url(host: &str, repo: &str, git_ref: &str, file: &str, authed: bool) -> String {
    if !authed && host == "github.com" {
        format!(
            "https://raw.githubusercontent.com/{repo}/{}/{}",
            encode_seg(git_ref),
            encode_path(file.trim_start_matches('/'))
        )
    } else {
        api_content_url(host, repo, git_ref, file)
    }
}

fn http_get_text(url: &str, token: Option<&str>, raw: bool) -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("trawl-plugin-installer")
        .build()
        .map_err(|e| e.to_string())?;
    let accept = if raw { "application/vnd.github.raw" } else { "application/vnd.github+json" };
    let mut req = client.get(url).header("Accept", accept);
    if let Some(t) = token {
        req = req.header("Authorization", format!("Bearer {t}"));
    }
    let resp = req.send().map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        let status = resp.status();
        // GitHub explains failures ("API rate limit exceeded…") in a JSON
        // `message`; surface it instead of a bare status code.
        let body = resp.text().unwrap_or_default();
        let msg = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(str::to_string))
            .unwrap_or_else(|| body.chars().take(200).collect::<String>().trim().to_string());
        return Err(if msg.is_empty() {
            format!("HTTP {status} for {url}")
        } else {
            format!("HTTP {status} for {url}: {msg}")
        });
    }
    resp.text().map_err(|e| e.to_string())
}

// ── Per-host git tokens (git-hosts.json) ──

fn git_hosts_path(data_dir: &Path) -> PathBuf {
    data_dir.join("git-hosts.json")
}

pub fn load_git_hosts(data_dir: &Path) -> HashMap<String, String> {
    fs::read_to_string(git_hosts_path(data_dir))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn host_token(data_dir: &Path, host: &str) -> Option<String> {
    load_git_hosts(data_dir).get(host).cloned()
}

/// An empty token removes the host's entry.
pub fn save_git_host_token(data_dir: &Path, host: &str, token: &str) -> Result<()> {
    fs::create_dir_all(data_dir).context("create data dir")?;
    let mut hosts = load_git_hosts(data_dir);
    if token.trim().is_empty() {
        hosts.remove(host);
    } else {
        hosts.insert(host.to_string(), token.trim().to_string());
    }
    fs::write(git_hosts_path(data_dir), serde_json::to_string_pretty(&hosts)?)
        .context("write git-hosts.json")?;
    Ok(())
}

fn fetch_manifest_blocking(
    host: &str,
    repo: &str,
    git_ref: &str,
    token: Option<&str>,
) -> Result<PluginManifest, String> {
    let text = http_get_text(
        &content_url(host, repo, git_ref, "trawl-plugin.json", token.is_some()),
        token,
        true,
    )?;
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

/// Raw URL of the public plugin catalog (YAML). Updatable without an app release.
const CATALOG_URL: &str =
    "https://raw.githubusercontent.com/legostin/trawl/main/plugins.yaml";

/// Fetch the public plugin catalog as raw YAML text (parsed in the frontend).
#[tauri::command]
pub async fn fetch_plugin_catalog() -> Result<String, String> {
    tokio::task::spawn_blocking(|| {
        let client = reqwest::blocking::Client::builder()
            .user_agent("trawl-plugin-installer")
            .build()
            .map_err(|e| e.to_string())?;
        let resp = client.get(CATALOG_URL).send().map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {} for plugin catalog", resp.status()));
        }
        resp.text().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn fetch_plugin_manifest(
    app: AppHandle,
    repo: String,
    reference: Option<String>,
    host: Option<String>,
) -> Result<PluginManifest, String> {
    let (parsed_host, repo, git_ref) = normalize_repo(&repo, reference.as_deref());
    let host = effective_host(parsed_host, host);
    let token = host_token(&data_dir(&app)?, &host);
    tokio::task::spawn_blocking(move || {
        fetch_manifest_blocking(&host, &repo, &git_ref, token.as_deref())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Install one plugin and, recursively, its manifest dependencies. `visited`
/// keys are "host/repo" — a cycle or duplicate dep is skipped, not an error.
fn install_tree(
    data: &Path,
    host: &str,
    repo: &str,
    git_ref: &str,
    host_api_version: Option<&str>,
    visited: &mut std::collections::HashSet<String>,
) -> Result<(), String> {
    if !visited.insert(format!("{host}/{repo}")) {
        return Ok(());
    }
    if visited.len() > 8 {
        return Err("dependency chain too deep".into());
    }
    let token = host_token(data, host);
    let manifest = fetch_manifest_blocking(host, repo, git_ref, token.as_deref())?;
    if manifest.id.trim().is_empty() {
        return Err("manifest is missing an id".into());
    }
    let display = if manifest.name.is_empty() { &manifest.id } else { &manifest.name };
    check_api_version(display, &manifest.api_version, host_api_version)?;
    let bundle = http_get_text(
        &content_url(host, repo, git_ref, &manifest.entry, token.is_some()),
        token.as_deref(),
        true,
    )?;

    // Cache the bundle.
    let dir = plugins_dir(data).join(&manifest.id);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    fs::write(dir.join("plugin.js"), &bundle).map_err(|e| e.to_string())?;

    // Upsert the registry entry.
    let mut file = load_plugins(data).map_err(|e| e.to_string())?;
    let plugin = Plugin {
        id: manifest.id.clone(),
        name: if manifest.name.is_empty() { manifest.id.clone() } else { manifest.name.clone() },
        version: manifest.version.clone(),
        description: manifest.description.clone(),
        author: manifest.author.clone(),
        repo: repo.to_string(),
        host: host.to_string(),
        git_ref: git_ref.to_string(),
        enabled: true,
        api_version: manifest.api_version.clone(),
    };
    if let Some(e) = file.plugins.iter_mut().find(|p| p.id == plugin.id) {
        *e = plugin;
    } else {
        file.plugins.push(plugin);
    }
    save_plugins(data, &file).map_err(|e| e.to_string())?;

    // Dependencies: install missing ones, refresh ones older than min_version.
    for dep in &manifest.dependencies {
        let existing = file.plugins.iter().find(|p| p.id == dep.id).cloned();
        let needed = match &existing {
            None => true,
            Some(p) => dep
                .min_version
                .as_deref()
                .is_some_and(|mv| cmp_versions(&p.version, mv) == std::cmp::Ordering::Less),
        };
        if needed {
            let dep_ref = dep.reference.clone().unwrap_or_else(|| "main".to_string());
            install_tree(data, &dep.host, &dep.repo, &dep_ref, host_api_version, visited)
                .map_err(|e| format!("dependency \"{}\": {e}", dep.id))?;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn install_plugin(
    app: AppHandle,
    repo: String,
    reference: Option<String>,
    host: Option<String>,
    host_api_version: Option<String>,
) -> Result<Vec<Plugin>, String> {
    let (parsed_host, repo, git_ref) = normalize_repo(&repo, reference.as_deref());
    let host = effective_host(parsed_host, host);
    let data = data_dir(&app)?;
    tokio::task::spawn_blocking(move || {
        let mut visited = std::collections::HashSet::new();
        install_tree(&data, &host, &repo, &git_ref, host_api_version.as_deref(), &mut visited)?;
        load_plugins(&data).map(|f| f.plugins).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
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
pub fn git_host_token_set(app: AppHandle, host: String, token: String) -> Result<(), String> {
    save_git_host_token(&data_dir(&app)?, &host, &token).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn git_host_token_has(app: AppHandle, host: String) -> Result<bool, String> {
    Ok(host_token(&data_dir(&app)?, &host).is_some())
}

/// Hand a stored host token to a plugin. Plugins already run with full access
/// to the app (see the PluginsPanel warning), so browsing logic can live in
/// plugins while tokens are entered once at install time.
#[tauri::command]
pub fn git_host_token_get(app: AppHandle, host: String) -> Result<Option<String>, String> {
    Ok(host_token(&data_dir(&app)?, &host))
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
    fn unauthenticated_github_fetch_uses_raw_host() {
        // No token → raw.githubusercontent.com, which has no 60 req/h API quota.
        assert_eq!(
            content_url("github.com", "o/r", "main", "trawl-plugin.json", false),
            "https://raw.githubusercontent.com/o/r/main/trawl-plugin.json"
        );
        assert_eq!(
            content_url("github.com", "o/r", "feat/x", "dist/plugin.js", false),
            "https://raw.githubusercontent.com/o/r/feat%2Fx/dist/plugin.js"
        );
    }

    #[test]
    fn authenticated_or_enterprise_fetch_uses_contents_api() {
        assert_eq!(
            content_url("github.com", "o/r", "main", "trawl-plugin.json", true),
            "https://api.github.com/repos/o/r/contents/trawl-plugin.json?ref=main"
        );
        // GHE keeps the API even without a token: it has no shared public
        // rate-limit problem and raw paths differ per instance config.
        assert_eq!(
            content_url("github.example.org", "o/r", "main", "trawl-plugin.json", false),
            "https://github.example.org/api/v3/repos/o/r/contents/trawl-plugin.json?ref=main"
        );
    }

    #[test]
    fn http_error_includes_response_body_message() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = [0u8; 2048];
            let _ = s.read(&mut buf);
            let body = r#"{"message":"API rate limit exceeded for 1.2.3.4.","documentation_url":"x"}"#;
            let resp = format!(
                "HTTP/1.1 403 Forbidden\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = s.write_all(resp.as_bytes());
        });
        let err = http_get_text(&format!("http://{addr}/x"), None, true).unwrap_err();
        assert!(err.contains("403"), "status kept: {err}");
        assert!(err.contains("API rate limit exceeded"), "body message surfaced: {err}");
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
    fn encodes_cyrillic_and_spaces_in_paths() {
        assert_eq!(
            encode_path("KLIA-11871 недавние поиски/GET_v4_adverts.json"),
            "KLIA-11871%20%D0%BD%D0%B5%D0%B4%D0%B0%D0%B2%D0%BD%D0%B8%D0%B5%20%D0%BF%D0%BE%D0%B8%D1%81%D0%BA%D0%B8/GET_v4_adverts.json"
        );
        assert_eq!(encode_seg("KL-31600-kaspi-flow"), "KL-31600-kaspi-flow");
        assert_eq!(
            encode_seg("ветка/с слэшем"),
            "%D0%B2%D0%B5%D1%82%D0%BA%D0%B0%2F%D1%81%20%D1%81%D0%BB%D1%8D%D1%88%D0%B5%D0%BC"
        );
    }

    #[test]
    fn manifest_dependencies_parse_with_defaults() {
        let m: PluginManifest = serde_json::from_str(
            r#"{
                "id": "contracts", "name": "Contracts", "entry": "dist/plugin.js",
                "dependencies": [
                    { "id": "http-client", "repo": "legostin/trawl-plugin-http-client", "minVersion": "0.3.1" }
                ]
            }"#,
        )
        .unwrap();
        assert_eq!(m.dependencies.len(), 1);
        let d = &m.dependencies[0];
        assert_eq!(d.host, "github.com");
        assert_eq!(d.reference, None);
        assert_eq!(d.min_version.as_deref(), Some("0.3.1"));
        // Manifests without the field keep working.
        let m2: PluginManifest =
            serde_json::from_str(r#"{ "id": "a", "name": "A", "entry": "e.js" }"#).unwrap();
        assert!(m2.dependencies.is_empty());
    }

    #[test]
    fn api_gate_blocks_manifests_newer_than_host() {
        // Plugin needs a newer host API than the app provides → clear error.
        let err = check_api_version("Notifications", "1.7.0", Some("1.6.0")).unwrap_err();
        assert!(err.contains("1.7.0"), "error should name the required version: {err}");
        assert!(err.contains("1.6.0"), "error should name the app's version: {err}");
        // Equal or older requirement is fine.
        assert!(check_api_version("X", "1.6.0", Some("1.6.0")).is_ok());
        assert!(check_api_version("X", "1.5.0", Some("1.6.0")).is_ok());
        // Manifests without apiVersion keep installing (pre-gate plugins).
        assert!(check_api_version("X", "", Some("1.6.0")).is_ok());
        // No host version supplied (old frontend) → no gate.
        assert!(check_api_version("X", "1.7.0", None).is_ok());
    }

    #[test]
    fn registry_defaults_api_version_for_old_files() {
        // plugins.json written before the gate has no apiVersion field.
        let p: Plugin = serde_json::from_str(
            r#"{ "id": "a", "name": "A", "version": "1.0.0", "description": "",
                 "author": "", "repo": "o/r", "ref": "main", "enabled": true }"#,
        )
        .unwrap();
        assert_eq!(p.api_version, "");
    }

    #[test]
    fn version_compare_is_numeric() {
        use std::cmp::Ordering::*;
        assert_eq!(cmp_versions("0.3.1", "0.3.1"), Equal);
        assert_eq!(cmp_versions("0.10.0", "0.9.9"), Greater);
        assert_eq!(cmp_versions("0.3.0", "0.3.1"), Less);
        assert_eq!(cmp_versions("1.0", "1.0.0"), Equal);
    }

    #[test]
    fn git_hosts_roundtrip() {
        let dir = std::env::temp_dir().join(format!("trawl-git-hosts-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        assert!(host_token(&dir, "github.example.org").is_none());
        save_git_host_token(&dir, "github.example.org", "tok123").unwrap();
        assert_eq!(host_token(&dir, "github.example.org").as_deref(), Some("tok123"));
        // Empty token removes the entry.
        save_git_host_token(&dir, "github.example.org", "").unwrap();
        assert!(host_token(&dir, "github.example.org").is_none());
        let _ = fs::remove_dir_all(&dir);
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
                api_version: "1.6.0".into(),
            }],
        };
        save_plugins(&dir, &file).unwrap();
        let back = load_plugins(&dir).unwrap();
        assert_eq!(back.plugins.len(), 1);
        assert_eq!(back.plugins[0].id, "a");
        let _ = fs::remove_dir_all(&dir);
    }
}
