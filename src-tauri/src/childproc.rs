//! Child processes started by plugins.
//!
//! A GUI `.app` inherits a minimal PATH, so `npx`/`node` are usually missing.
//! The user's real PATH is resolved once from their login shell and reused for
//! every spawn. Processes are owned by the plugin that started them so they can
//! be killed when it is disabled, reloaded, or the app exits.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

#[derive(Debug, Clone, Serialize)]
pub struct ProcessInfo {
    pub id: String,
    pub pid: u32,
    pub plugin_id: String,
    pub command: String,
    pub started_at: u64,
}

#[derive(Debug, Clone, Serialize)]
struct OutputEvent {
    id: String,
    stream: &'static str,
    text: String,
}

#[derive(Debug, Clone, Serialize)]
struct ExitEvent {
    id: String,
    code: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct SpawnRequest {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

struct Entry {
    info: ProcessInfo,
    child: Child,
}

#[derive(Default)]
pub struct ProcState {
    entries: Mutex<Vec<Entry>>,
}

impl ProcState {
    pub fn new() -> Self {
        Self::default()
    }
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static LOGIN_PATH: OnceLock<Option<String>> = OnceLock::new();

/// The PATH a terminal would have. Resolved once by asking the login shell;
/// `None` when that fails, in which case the inherited PATH is used as-is.
pub fn login_path() -> Option<String> {
    LOGIN_PATH
        .get_or_init(|| {
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
            let out = std::process::Command::new(&shell)
                .args(["-lc", "printf %s \"$PATH\""])
                .output()
                .ok()?;
            let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if path.is_empty() {
                None
            } else {
                Some(path)
            }
        })
        .clone()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Start a process owned by `plugin_id`, streaming its output as events.
pub fn spawn(
    app: &AppHandle,
    state: &ProcState,
    plugin_id: &str,
    req: SpawnRequest,
) -> Result<ProcessInfo> {
    let mut cmd = Command::new(&req.command);
    cmd.args(&req.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    if let Some(path) = login_path() {
        cmd.env("PATH", path);
    }
    for (key, value) in &req.env {
        cmd.env(key, value);
    }
    if let Some(cwd) = &req.cwd {
        cmd.current_dir(cwd);
    }

    let mut child = cmd.spawn()?;
    let id = format!("p_{}", NEXT_ID.fetch_add(1, Ordering::Relaxed));
    let pid = child.id().unwrap_or(0);

    if let Some(stdout) = child.stdout.take() {
        pump(app.clone(), id.clone(), "stdout", stdout);
    }
    if let Some(stderr) = child.stderr.take() {
        pump(app.clone(), id.clone(), "stderr", stderr);
    }

    let info = ProcessInfo {
        id: id.clone(),
        pid,
        plugin_id: plugin_id.to_string(),
        command: std::iter::once(req.command.clone())
            .chain(req.args.iter().cloned())
            .collect::<Vec<_>>()
            .join(" "),
        started_at: now_ms(),
    };

    state
        .entries
        .lock()
        .unwrap()
        .push(Entry { info: info.clone(), child });
    Ok(info)
}

fn pump<R>(app: AppHandle, id: String, stream: &'static str, reader: R)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tauri::async_runtime::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(text)) = lines.next_line().await {
            let _ = app.emit(
                "plugin-process-output",
                OutputEvent {
                    id: id.clone(),
                    stream,
                    text,
                },
            );
        }
    });
}

/// Processes still running for a plugin, pruning the ones that already exited.
pub fn list(app: &AppHandle, state: &ProcState, plugin_id: Option<&str>) -> Vec<ProcessInfo> {
    let mut entries = state.entries.lock().unwrap();
    let mut alive = Vec::new();
    let mut out = Vec::new();

    for mut entry in entries.drain(..) {
        match entry.child.try_wait() {
            Ok(Some(status)) => {
                let _ = app.emit(
                    "plugin-process-exit",
                    ExitEvent {
                        id: entry.info.id.clone(),
                        code: status.code(),
                    },
                );
            }
            _ => {
                if plugin_id.is_none_or(|p| p == entry.info.plugin_id) {
                    out.push(entry.info.clone());
                }
                alive.push(entry);
            }
        }
    }
    *entries = alive;
    out
}

/// Kill one process. Unknown ids are a no-op.
pub fn kill(state: &ProcState, id: &str) {
    let mut entries = state.entries.lock().unwrap();
    if let Some(at) = entries.iter().position(|e| e.info.id == id) {
        let mut entry = entries.remove(at);
        let _ = futures_kill(&mut entry.child);
    }
}

/// Kill everything a plugin started (disable, reload, uninstall).
pub fn kill_plugin(state: &ProcState, plugin_id: &str) {
    let mut entries = state.entries.lock().unwrap();
    let (mine, others): (Vec<_>, Vec<_>) = entries
        .drain(..)
        .partition(|e| e.info.plugin_id == plugin_id);
    *entries = others;
    for mut entry in mine {
        let _ = futures_kill(&mut entry.child);
    }
}

/// Kill everything (app exit).
pub fn kill_all(state: &ProcState) {
    let mut entries = state.entries.lock().unwrap();
    for mut entry in entries.drain(..) {
        let _ = futures_kill(&mut entry.child);
    }
}

fn futures_kill(child: &mut Child) -> Result<()> {
    child.start_kill()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_path_is_resolved_or_absent() {
        // Either the login shell answered, or we fall back — never an empty string.
        assert!(login_path().map(|p| !p.is_empty()).unwrap_or(true));
    }

    #[test]
    fn spawn_request_defaults_are_empty() {
        let req: SpawnRequest = serde_json::from_str(r#"{"command":"echo"}"#).unwrap();
        assert_eq!(req.command, "echo");
        assert!(req.args.is_empty());
        assert!(req.env.is_empty());
        assert!(req.cwd.is_none());
    }
}

// ---- Tauri commands -------------------------------------------------------

use tauri::State;

#[tauri::command]
pub fn plugin_spawn(
    app: AppHandle,
    state: State<'_, ProcState>,
    plugin_id: String,
    request: SpawnRequest,
) -> Result<ProcessInfo, String> {
    spawn(&app, &state, &plugin_id, request).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn plugin_kill_process(state: State<'_, ProcState>, id: String) {
    kill(&state, &id);
}

#[tauri::command]
pub fn plugin_list_processes(
    app: AppHandle,
    state: State<'_, ProcState>,
    plugin_id: Option<String>,
) -> Vec<ProcessInfo> {
    list(&app, &state, plugin_id.as_deref())
}

#[tauri::command]
pub fn plugin_kill_processes(state: State<'_, ProcState>, plugin_id: String) {
    kill_plugin(&state, &plugin_id);
}
