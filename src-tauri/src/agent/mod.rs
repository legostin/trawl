pub mod claude;
pub mod events;
pub mod harness;
pub mod mcp_config;

use events::AgentEvent;
use std::io::{BufRead, BufReader, Write};
// `.process_group(0)` is an extension trait, exactly as in childproc.rs.
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};

/// What survives between messages: only the harness's own session id. The
/// transcript lives in the UI, so a crashed process loses no conversation.
#[derive(Default)]
pub struct Session {
    session_id: Mutex<Option<String>>,
}

impl Session {
    pub fn observe(&self, event: &AgentEvent) {
        if let AgentEvent::SessionStarted { session_id, .. } = event {
            *self.session_id.lock().unwrap() = Some(session_id.clone());
        }
    }

    pub fn resume_arg(&self) -> Option<String> {
        self.session_id.lock().unwrap().clone()
    }

    pub fn reset(&self) {
        *self.session_id.lock().unwrap() = None;
    }
}

#[derive(Default)]
pub struct AgentState {
    pub session: Session,
    /// The turn in flight, so a second send can be refused and an interrupt has
    /// something to kill.
    pub running: Mutex<Option<std::process::Child>>,
}

impl AgentState {
    pub fn new() -> Self {
        Self::default()
    }
}

fn emit(app: &AppHandle, event: &AgentEvent) {
    let _ = app.emit("agent-event", event);
}

/// The harness talks to Trawl through our MCP server, so the server has to be
/// up. If the user has it switched off we start it for this session only — the
/// saved config is left alone, and `SessionStarted.trawl_connected` tells the
/// panel whether it actually worked.
async fn ensure_mcp_running(app: &AppHandle) -> Result<crate::mcp::McpConfig, String> {
    let cfg = crate::mcp::load_config(&crate::commands::data_dir(app)?);
    let already = {
        let mcp = app.state::<crate::mcp::McpState>();
        let server = mcp.server.lock().unwrap();
        server.is_some()
    };
    if !already {
        let mut session_cfg = cfg.clone();
        session_cfg.enabled = true;
        crate::mcp::apply_config(app, &session_cfg).await;
    }
    Ok(cfg)
}

/// Sends one user message and streams the harness's answer as `agent-event`s.
/// Returns once the process is running; the turn completes in the pump thread.
#[tauri::command]
pub async fn agent_send(
    app: AppHandle,
    text: String,
    screen_context: String,
) -> Result<(), String> {
    {
        let state = app.state::<AgentState>();
        let running = state.running.lock().unwrap();
        if running.is_some() {
            return Err("the agent is still working on the previous message".into());
        }
    }

    let cfg = ensure_mcp_running(&app).await?;

    // app_data_dir() is what every other module here uses; do not introduce a
    // second location for app state.
    let dir = crate::commands::data_dir(&app)?.join("agent");
    let mcp_path = mcp_config::write_mcp_config_file(&dir, cfg.port, &cfg.token)
        .map_err(|e| format!("could not write the MCP config: {e}"))?;

    let launch = claude::LaunchConfig {
        resume: app.state::<AgentState>().session.resume_arg(),
        cwd: dir.to_string_lossy().to_string(),
        mcp_config_path: mcp_path.to_string_lossy().to_string(),
        system_prompt: SYSTEM_PROMPT.to_string(),
    };

    let mut cmd = Command::new("claude");
    cmd.args(claude::build_args(&launch))
        .current_dir(&launch.cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    if let Some(path) = crate::childproc::login_path() {
        cmd.env("PATH", path);
    }

    let mut child = cmd.spawn().map_err(|e| {
        format!("could not start claude: {e}. Is Claude Code installed and on your PATH?")
    })?;

    // One user message per turn, then EOF: `-p` reads until stdin closes.
    let payload = serde_json::json!({
        "type": "user",
        "message": { "role": "user", "content": format!("{screen_context}\n\n{text}") }
    });
    let mut stdin = child.stdin.take().ok_or("claude gave us no stdin")?;
    writeln!(stdin, "{payload}").map_err(|e| e.to_string())?;
    drop(stdin);

    let stdout = child.stdout.take().ok_or("claude gave us no stdout")?;
    *app.state::<AgentState>().running.lock().unwrap() = Some(child);

    let app2 = app.clone();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            for event in claude::parse_line(&line) {
                app2.state::<AgentState>().session.observe(&event);
                emit(&app2, &event);
            }
        }
        let child = app2.state::<AgentState>().running.lock().unwrap().take();
        if let Some(mut c) = child {
            let _ = c.wait();
        }
    });
    Ok(())
}

#[tauri::command]
pub fn agent_interrupt(state: State<'_, AgentState>) {
    if let Some(mut c) = state.running.lock().unwrap().take() {
        let _ = c.kill();
    }
}

#[tauri::command]
pub fn agent_reset(state: State<'_, AgentState>) {
    if let Some(mut c) = state.running.lock().unwrap().take() {
        let _ = c.kill();
    }
    state.session.reset();
}

#[tauri::command]
pub fn agent_status(state: State<'_, AgentState>) -> serde_json::Value {
    serde_json::json!({
        "running": state.running.lock().unwrap().is_some(),
        "sessionId": state.session.resume_arg(),
    })
}

const SYSTEM_PROMPT: &str = "\
You are working inside Trawl, an HTTP(S) proxy inspector. The user is looking at \
its UI; each message begins with a <screen> block saying where they are and what \
is selected. That block holds pointers, not data — use the trawl MCP tools to \
fetch what you need.

Traffic queries are already confined to the active project by the server, so do \
not widen them by hand. The screen block also says when the current session \
began: prefer traffic from it, and reach further back only when the question is \
plainly about history.

You can change Trawl's own state: rules, breakpoints and projects. Those changes \
take effect immediately and the user sees no confirmation prompt, so make the \
change they asked for and say what you did. You have no access to their files or \
a shell; if something genuinely needs one, say so instead of pretending.";

#[cfg(test)]
mod tests {
    use super::*;

    fn started(id: &str) -> AgentEvent {
        AgentEvent::SessionStarted {
            session_id: id.into(),
            model: "m".into(),
            trawl_connected: true,
        }
    }

    #[test]
    fn the_session_id_from_init_is_what_the_next_message_resumes() {
        let s = Session::default();
        assert_eq!(s.resume_arg(), None);
        s.observe(&started("sess-9"));
        assert_eq!(s.resume_arg(), Some("sess-9".to_string()));
    }

    #[test]
    fn a_reset_forgets_the_session_so_the_next_message_starts_fresh() {
        let s = Session::default();
        s.observe(&started("sess-9"));
        s.reset();
        assert_eq!(s.resume_arg(), None);
    }
}
