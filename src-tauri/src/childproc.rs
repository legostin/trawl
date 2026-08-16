//! Child processes started by plugins.
//!
//! A GUI `.app` inherits a minimal PATH, so `npx`/`node` are usually missing.
//! The user's real PATH is resolved once from their login shell and reused for
//! every spawn. Processes are owned by the plugin that started them so they can
//! be killed when it is disabled, reloaded, or the app exits.
//!
//! Deliberately `std::process` rather than `tokio::process`: these calls arrive
//! from Tauri commands and from the app's exit handler, none of which are
//! guaranteed to run inside the tokio runtime — and tokio's child processes
//! panic when touched without a reactor.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

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
    /// Plugins the user has agreed may start programs, for this run of the app.
    ///
    /// Kept here rather than in the webview: a plugin shares the page with the
    /// host, so a grant stored in `localStorage` is one the grantee can write
    /// for itself.
    spawn_granted: Mutex<std::collections::HashSet<String>>,
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
            let out = Command::new(&shell)
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
        // Its own process group, so stopping it stops the whole tree. `npx`
        // execs node, which runs a browser: killing only the wrapper leaves
        // both behind.
        .process_group(0);

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
    let pid = child.id();

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

    state.entries.lock().unwrap().push(Entry {
        info: info.clone(),
        child,
    });
    Ok(info)
}

fn pump<R: Read + Send + 'static>(app: AppHandle, id: String, stream: &'static str, reader: R) {
    std::thread::spawn(move || {
        for line in BufReader::new(reader).lines() {
            let Ok(text) = line else { break };
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

fn stop(entry: &mut Entry) {
    // The group first: children of the spawned command (node, browsers) are in
    // it, and they are what the user sees left behind.
    let pid = entry.child.id() as i32;
    unsafe {
        libc::killpg(pid, libc::SIGTERM);
    }
    // Give the tree a moment to shut down cleanly, then insist.
    for _ in 0..20 {
        match entry.child.try_wait() {
            Ok(Some(_)) => return,
            _ => std::thread::sleep(std::time::Duration::from_millis(50)),
        }
    }
    unsafe {
        libc::killpg(pid, libc::SIGKILL);
    }
    let _ = entry.child.kill();
    let _ = entry.child.wait(); // reap, so no zombie is left behind
}

/// Kill one process. Unknown ids are a no-op.
pub fn kill(state: &ProcState, id: &str) {
    let mut entries = state.entries.lock().unwrap();
    if let Some(at) = entries.iter().position(|e| e.info.id == id) {
        let mut entry = entries.remove(at);
        stop(&mut entry);
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
        stop(&mut entry);
    }
}

/// Kill everything (app exit).
pub fn kill_all(state: &ProcState) {
    let mut entries = state.entries.lock().unwrap();
    for mut entry in entries.drain(..) {
        stop(&mut entry);
    }
}

// ---- Tauri commands -------------------------------------------------------

/// Asks the user, once per plugin per app run, before it may start programs.
///
/// The prompt is a native dialog on purpose. An in-app modal is drawn by the
/// same page the plugin runs in, so a plugin can dismiss it, pre-approve itself
/// or answer the event on the user's behalf. A window the OS owns is the only
/// one page script cannot click.
async fn allowed_to_spawn(app: &AppHandle, state: &ProcState, plugin_id: &str, cmd: &str) -> bool {
    if state
        .spawn_granted
        .lock()
        .unwrap()
        .contains(plugin_id)
    {
        return true;
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    tauri_plugin_dialog::DialogExt::dialog(app)
        .message(format!(
            "The plugin \"{plugin_id}\" wants to run a program on your machine, with your permissions:\n\n{cmd}\n\nAllow it for the rest of this session?"
        ))
        .title("Trawl — a plugin wants to run a program")
        .buttons(tauri_plugin_dialog::MessageDialogButtons::OkCancelCustom(
            "Allow".into(),
            "Deny".into(),
        ))
        .show(move |granted| {
            let _ = tx.send(granted);
        });
    if rx.await.unwrap_or(false) {
        state
            .spawn_granted
            .lock()
            .unwrap()
            .insert(plugin_id.to_string());
        true
    } else {
        false
    }
}

#[tauri::command]
pub async fn plugin_spawn(
    app: AppHandle,
    state: State<'_, ProcState>,
    plugin_id: String,
    request: SpawnRequest,
) -> Result<ProcessInfo, String> {
    let shown = std::iter::once(request.command.clone())
        .chain(request.args.iter().cloned())
        .collect::<Vec<_>>()
        .join(" ");
    if !allowed_to_spawn(&app, &state, &plugin_id, &shown).await {
        return Err(format!("\"{plugin_id}\" was not allowed to run a program"));
    }
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

    /// Stopping a wrapper must take its children with it: the agent is `npx`,
    /// which execs node, which runs a browser — killing only the wrapper is
    /// exactly how a browser gets left on screen.
    #[test]
    fn stop_takes_the_whole_process_tree() {
        // A shell that spawns a long-lived grandchild and waits.
        let mut child = Command::new("/bin/sh")
            .args(["-c", "sleep 300 & echo $!; wait"])
            .stdout(Stdio::piped())
            .process_group(0)
            .spawn()
            .expect("spawn");

        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let grandchild: i32 = line.trim().parse().expect("grandchild pid");
        assert_eq!(unsafe { libc::kill(grandchild, 0) }, 0, "grandchild should be alive");

        let mut entry = Entry {
            info: ProcessInfo {
                id: "p_tree".into(),
                pid: child.id(),
                plugin_id: "test".into(),
                command: "sh".into(),
                started_at: now_ms(),
            },
            child,
        };
        stop(&mut entry);

        std::thread::sleep(std::time::Duration::from_millis(200));
        assert_eq!(
            unsafe { libc::kill(grandchild, 0) },
            -1,
            "the grandchild must be gone, not orphaned",
        );
    }

    /// The panic this module exists to avoid: spawning from a plain thread, with
    /// no tokio runtime anywhere in sight, must simply work.
    #[test]
    fn spawns_and_reaps_without_a_runtime() {
        let mut child = Command::new("/bin/sh")
            .args(["-c", "printf hello; sleep 5"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn");

        let mut buf = [0u8; 5];
        child.stdout.as_mut().unwrap().read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"hello");

        let mut entry = Entry {
            info: ProcessInfo {
                id: "p_test".into(),
                pid: child.id(),
                plugin_id: "test".into(),
                command: "sh".into(),
                started_at: now_ms(),
            },
            child,
        };
        stop(&mut entry);
        assert!(entry.child.try_wait().unwrap().is_some(), "child should be reaped");
    }
}

#[cfg(test)]
mod signal_tests {
    use super::*;

    /// The app is often stopped by a signal rather than by closing its window,
    /// and a plugin's programs must not survive that. This covers the piece the
    /// signal thread calls; the wiring itself lives in lib.rs.
    #[test]
    fn kill_all_reaps_every_registered_child() {
        let state = ProcState::new();
        let mut pids = Vec::new();

        for _ in 0..3 {
            let child = Command::new("/bin/sh")
                .args(["-c", "sleep 300"])
                .process_group(0)
                .spawn()
                .expect("spawn");
            pids.push(child.id() as i32);
            state.entries.lock().unwrap().push(Entry {
                info: ProcessInfo {
                    id: format!("p_{}", child.id()),
                    pid: child.id(),
                    plugin_id: "test".into(),
                    command: "sleep".into(),
                    started_at: now_ms(),
                },
                child,
            });
        }
        assert!(pids.iter().all(|pid| unsafe { libc::kill(*pid, 0) } == 0));

        kill_all(&state);

        std::thread::sleep(std::time::Duration::from_millis(200));
        for pid in pids {
            assert_eq!(unsafe { libc::kill(pid, 0) }, -1, "child {pid} outlived the app");
        }
        assert!(state.entries.lock().unwrap().is_empty());
    }
}
