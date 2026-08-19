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

/// Markers around the answer, so anything an rc file prints on its way up —
/// version-manager banners, MOTDs, `nvm use` chatter — cannot be mistaken for
/// a PATH.
const PATH_BEGIN: &str = "__TRAWL_PATH_BEGIN__";
const PATH_END: &str = "__TRAWL_PATH_END__";

/// An rc file can hang (waiting on input, a slow network mount, a prompt
/// framework). Detection must not hang with it.
const SHELL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);

fn extract_path(stdout: &str) -> Option<String> {
    let start = stdout.find(PATH_BEGIN)? + PATH_BEGIN.len();
    let rest = &stdout[start..];
    let path = rest[..rest.find(PATH_END)?].trim();
    (!path.is_empty()).then(|| path.to_string())
}

/// Ask `shell` for the PATH it would give a person at a terminal.
///
/// `-i` is the load-bearing flag: `.zshrc` and `.bashrc` are read **only** by
/// interactive shells, and that is where `nvm`, `~/.local/bin` and
/// `~/.npm-global/bin` are normally added. A GUI app has no terminal PATH to
/// fall back on, so a non-interactive login shell silently loses all of them.
fn shell_path(shell: &str, args: &[&str], extra_env: &[(&str, &str)]) -> Option<String> {
    let script = format!("printf %s%s%s '{PATH_BEGIN}' \"$PATH\" '{PATH_END}'");
    let mut cmd = Command::new(shell);
    cmd.args(args)
        .arg(script)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        // Its own group, so a hung rc file's children die with it.
        .process_group(0);
    for (key, value) in extra_env {
        cmd.env(key, value);
    }

    let child = cmd.spawn().ok()?;
    let pid = child.id() as i32;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    match rx.recv_timeout(SHELL_TIMEOUT) {
        Ok(Ok(out)) => extract_path(&String::from_utf8_lossy(&out.stdout)),
        Ok(Err(_)) => None,
        Err(_) => {
            unsafe { libc::kill(-pid, libc::SIGKILL) };
            None
        }
    }
}

/// The PATH a terminal would have. Resolved once by asking the login shell;
/// `None` when that fails, in which case the inherited PATH is used as-is.
pub fn login_path() -> Option<String> {
    LOGIN_PATH
        .get_or_init(|| {
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
            // A non-interactive login shell is the fallback, not the answer: it
            // misses `.zshrc`, but an rc file that refuses to run interactively
            // should still leave us better off than the app's own PATH.
            shell_path(&shell, &["-ilc"], &[]).or_else(|| shell_path(&shell, &["-lc"], &[]))
        })
        .clone()
}

/// Turn a bare program name into an absolute path.
///
/// `Command::new("claude")` resolves the name against the PATH of *this*
/// process; `cmd.env("PATH", …)` only populates the child's environment and has
/// no say in that lookup. A GUI app therefore fails to start a program it can
/// see perfectly well — the "could not start claude: No such file or directory"
/// people were getting. Resolving here keeps the launcher and the detector
/// looking in exactly the same places.
///
/// A name that is already a path is left alone, and an unknown one is returned
/// unchanged so the spawn error still names what was asked for.
pub fn resolve_program(name: &str) -> String {
    if name.contains('/') {
        return name.to_string();
    }
    let path = login_path().or_else(|| std::env::var("PATH").ok()).unwrap_or_default();
    crate::agent::harness::find_anywhere(name, &path, None).unwrap_or_else(|| name.to_string())
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
    let mut cmd = Command::new(resolve_program(&req.command));
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
    fn a_bare_program_name_is_resolved_to_an_absolute_path() {
        // `Command::new("claude")` searches the PATH of *this* process, and
        // `cmd.env("PATH", …)` only sets the child's environment — it does not
        // change that lookup. A GUI app therefore fails to start a program it
        // can see perfectly well, which is exactly what "could not start
        // claude: No such file or directory" was.
        let resolved = resolve_program("sh");
        assert!(
            resolved.starts_with('/'),
            "expected an absolute path, got {resolved}"
        );
    }

    #[test]
    fn a_path_that_is_already_a_path_is_left_alone() {
        assert_eq!(resolve_program("/bin/sh"), "/bin/sh");
        assert_eq!(resolve_program("./local-tool"), "./local-tool");
    }

    #[test]
    fn an_unknown_program_is_returned_unchanged_so_the_error_names_it() {
        assert_eq!(resolve_program("definitely-not-installed-xyz"), "definitely-not-installed-xyz");
    }

    #[test]
    fn login_path_is_resolved_or_absent() {
        // Either the login shell answered, or we fall back — never an empty string.
        assert!(login_path().map(|p| !p.is_empty()).unwrap_or(true));
    }

    #[test]
    fn the_answer_is_read_from_between_the_markers() {
        let out = format!("nvm: now using node v22\n{PATH_BEGIN}/opt/bin:/usr/bin{PATH_END}");
        assert_eq!(extract_path(&out), Some("/opt/bin:/usr/bin".into()));
    }

    #[test]
    fn chatter_without_markers_is_not_a_path() {
        // An rc file that greets the user must not be parsed as an answer.
        assert_eq!(extract_path("Welcome back!\n"), None);
        assert_eq!(extract_path(&format!("{PATH_BEGIN}  {PATH_END}")), None);
    }

    /// The bug this guards: `~/.zshrc` is read *only* by interactive shells, and
    /// that is where `nvm`, `~/.local/bin` and `~/.npm-global/bin` are usually
    /// added. Run from a terminal the app inherits those anyway, so a
    /// non-interactive login shell looks fine in dev and loses every one of them
    /// in a packaged `.app`.
    #[test]
    fn a_path_set_only_in_zshrc_is_found() {
        if !std::path::Path::new("/bin/zsh").exists() {
            return; // No zsh on this machine — nothing to prove.
        }
        let dir = tmpdir("zdotdir");
        std::fs::write(dir.join(".zshenv"), "").unwrap();
        std::fs::write(dir.join(".zprofile"), "").unwrap();
        std::fs::write(
            dir.join(".zshrc"),
            "export PATH=/opt/only-from-zshrc:$PATH\n",
        )
        .unwrap();
        let env = [("ZDOTDIR", dir.to_str().unwrap())];
        let has_it = |p: Option<String>| {
            p.unwrap_or_default()
                .split(':')
                .any(|d| d == "/opt/only-from-zshrc")
        };

        assert!(
            has_it(shell_path("/bin/zsh", &["-ilc"], &env)),
            "an interactive login shell must see .zshrc"
        );
        assert!(
            !has_it(shell_path("/bin/zsh", &["-lc"], &env)),
            "a non-interactive login shell never reads .zshrc — that was the bug"
        );
    }

    fn tmpdir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("trawl-childproc-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
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
