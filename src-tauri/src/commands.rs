use std::net::SocketAddr;
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::ca::load_or_create_ca;
use crate::db::{AggBucket, DbHandle, FlowQuery, FlowRow, Report};
use crate::model::Flow;
use crate::net::lan_ip;
use crate::projects::{self, Project, ProjectsFile};
use crate::proxy::{self, ProxyHandle};
use crate::rules::{self, Rule};
use crate::store::FlowStore;

pub struct AppState {
    pub store: FlowStore,
    pub proxy: Mutex<Option<ProxyHandle>>,
    /// Живой список правил, разделяемый с прокси-хендлером.
    pub rules: Arc<RwLock<Vec<Rule>>>,
    /// Live library-prelude, разделяемый с прокси-хендлером.
    pub library: Arc<RwLock<String>>,
    /// Активный проект (None = пишем всё). Разделяется с прокси-хендлером.
    pub active_project: Arc<RwLock<Option<Project>>>,
    pub scripts: crate::scripting::ScriptClient,
    /// Persistent flow DB (SQLite). Initialized once in the Tauri setup hook.
    pub db: OnceLock<DbHandle>,
    /// Live breakpoint definitions, shared with the proxy handler.
    pub breakpoints: Arc<RwLock<Vec<crate::breakpoints::Breakpoint>>>,
    /// Master intercept switch (enables/disables all breakpoints).
    pub intercept: Arc<RwLock<bool>>,
    /// Flows currently held on a breakpoint, keyed by (flow id, phase).
    pub pending_breakpoints: crate::proxy::BreakpointRegistry,
    /// Auto-continue a paused flow after N seconds (0 = hold forever).
    pub breakpoint_timeout: Arc<RwLock<u64>>,
    /// Hold new requests while any flow is paused on a breakpoint.
    pub pause_others: Arc<RwLock<bool>>,
}

impl AppState {
    pub fn new() -> Self {
        AppState {
            store: FlowStore::new(5000),
            proxy: Mutex::new(None),
            rules: Arc::new(RwLock::new(Vec::new())),
            library: Arc::new(RwLock::new(String::new())),
            active_project: Arc::new(RwLock::new(None)),
            scripts: crate::scripting::spawn_engine(
                std::time::Duration::from_secs(1),
                Arc::new(|name: &str| crate::secrets::get(name).ok().flatten()),
            ),
            db: OnceLock::new(),
            breakpoints: Arc::new(RwLock::new(Vec::new())),
            intercept: Arc::new(RwLock::new(true)),
            pending_breakpoints: Arc::new(Mutex::new(std::collections::HashMap::new())),
            breakpoint_timeout: Arc::new(RwLock::new(0)),
            pause_others: Arc::new(RwLock::new(false)),
        }
    }

    pub fn db(&self) -> Result<&DbHandle, String> {
        self.db.get().ok_or_else(|| "database not initialized".to_string())
    }
}

/// Open the flow DB and store the handle in `AppState` (called from the setup hook).
pub fn init_db(app: &AppHandle, state: &AppState) -> Result<(), String> {
    let path = data_dir(app)?.join("trawl.db");
    let handle = DbHandle::open(path).map_err(|e| e.to_string())?;
    let _ = state.db.set(handle);
    Ok(())
}

/// Generic over the Tauri runtime so it works both with the real webview
/// (production) and `tauri::test::MockRuntime` (used by the MCP server's
/// integration test, which is generic over `R: tauri::Runtime`).
pub fn data_dir<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<std::path::PathBuf, String> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

fn ca_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("ca"))
}

pub fn rules_dir<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("scripting"))
}

#[tauri::command]
pub async fn start_proxy(
    port: u16,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    if state.proxy.lock().unwrap().is_some() {
        return Err("proxy already running".into());
    }
    let addr: SocketAddr = format!("0.0.0.0:{port}")
        .parse()
        .map_err(|e: std::net::AddrParseError| e.to_string())?;
    let app_for_emit = app.clone();
    let emit: proxy::EmitFn = std::sync::Arc::new(move |event: &str, flow: &Flow| {
        let _ = app_for_emit.emit(event, flow.clone());
    });
    let app_for_notify = app.clone();
    let notify: proxy::NotifyFn = std::sync::Arc::new(move |payload: serde_json::Value| {
        let _ = app_for_notify.emit("script-notify", payload);
    });
    let secret_fn: crate::scripting::SecretFn =
        std::sync::Arc::new(|name: &str| crate::secrets::get(name).ok().flatten());
    // Подтянуть актуальные правила, библиотеку и активный проект перед стартом.
    let rdir = rules_dir(&app)?;
    let loaded_rules = rules::load_rules(&rdir).map_err(|e| e.to_string())?;
    let loaded_library = rules::load_library(&rdir).map_err(|e| e.to_string())?;
    let loaded_bps = crate::breakpoints::load_breakpoints(&rdir).map_err(|e| e.to_string())?;
    let bp_settings = crate::breakpoints::load_settings(&rdir);
    *state.rules.write().unwrap() = loaded_rules;
    *state.library.write().unwrap() = loaded_library;
    *state.breakpoints.write().unwrap() = loaded_bps;
    *state.breakpoint_timeout.write().unwrap() = bp_settings.timeout_secs;
    *state.pause_others.write().unwrap() = bp_settings.pause_others;
    let pfile = projects::load_projects(&data_dir(&app)?).map_err(|e| e.to_string())?;
    *state.active_project.write().unwrap() = pfile
        .active_id
        .and_then(|i| pfile.projects.into_iter().find(|p| p.id == i));

    let handle = proxy::start(
        addr,
        state.store.clone(),
        emit,
        notify,
        secret_fn,
        ca_dir(&app)?,
        state.scripts.clone(),
        state.rules.clone(),
        state.library.clone(),
        state.active_project.clone(),
        data_dir(&app)?,
        state.db.get().cloned(),
        state.breakpoints.clone(),
        state.intercept.clone(),
        state.pending_breakpoints.clone(),
        state.breakpoint_timeout.clone(),
        state.pause_others.clone(),
    )
    .await
    .map_err(|e| e.to_string())?;
    *state.proxy.lock().unwrap() = Some(handle);
    Ok(format!("0.0.0.0:{port}"))
}

#[tauri::command]
pub fn stop_proxy(state: State<'_, AppState>) -> Result<(), String> {
    if let Some(handle) = state.proxy.lock().unwrap().take() {
        handle.stop();
    }
    Ok(())
}

#[tauri::command]
pub fn get_flows(state: State<'_, AppState>) -> Vec<Flow> {
    state.store.all()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupInfo {
    pub lan_ip: Option<String>,
    pub port: u16,
    pub cert_host: String,
}

#[tauri::command]
pub fn get_setup_info() -> Result<SetupInfo, String> {
    Ok(SetupInfo {
        lan_ip: lan_ip().map(|ip| ip.to_string()),
        port: 8729,
        cert_host: "trawl".into(),
    })
}

#[tauri::command]
pub fn get_ca_pem(app: AppHandle) -> Result<String, String> {
    let mat = load_or_create_ca(&ca_dir(&app)?).map_err(|e| e.to_string())?;
    Ok(mat.cert_pem)
}

#[tauri::command]
pub fn ca_cert_path(app: AppHandle) -> Result<String, String> {
    let dir = ca_dir(&app)?;
    // гарантируем, что файл существует
    load_or_create_ca(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("ca.pem").to_string_lossy().to_string())
}

// ── Правила и библиотека скриптов ──

#[tauri::command]
pub fn list_rules(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<Rule>, String> {
    let loaded = rules::load_rules(&rules_dir(&app)?).map_err(|e| e.to_string())?;
    *state.rules.write().unwrap() = loaded.clone();
    Ok(loaded)
}

#[tauri::command]
pub fn save_rule(app: AppHandle, rule: Rule, state: State<'_, AppState>) -> Result<Vec<Rule>, String> {
    let rules = rules::upsert_rule(&rules_dir(&app)?, rule)?;
    *state.rules.write().unwrap() = rules.clone();
    Ok(rules)
}

#[tauri::command]
pub fn delete_rule(app: AppHandle, id: String, state: State<'_, AppState>) -> Result<Vec<Rule>, String> {
    let rules = rules::remove_rule(&rules_dir(&app)?, &id)?;
    *state.rules.write().unwrap() = rules.clone();
    Ok(rules)
}

// ── Брейкпоинты ──

#[tauri::command]
pub fn list_breakpoints(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<crate::breakpoints::Breakpoint>, String> {
    let loaded = crate::breakpoints::load_breakpoints(&rules_dir(&app)?).map_err(|e| e.to_string())?;
    *state.breakpoints.write().unwrap() = loaded.clone();
    Ok(loaded)
}

#[tauri::command]
pub fn save_breakpoint(
    app: AppHandle,
    breakpoint: crate::breakpoints::Breakpoint,
    state: State<'_, AppState>,
) -> Result<Vec<crate::breakpoints::Breakpoint>, String> {
    let bps = crate::breakpoints::upsert_breakpoint(&rules_dir(&app)?, breakpoint)?;
    *state.breakpoints.write().unwrap() = bps.clone();
    Ok(bps)
}

#[tauri::command]
pub fn delete_breakpoint(
    app: AppHandle,
    id: String,
    state: State<'_, AppState>,
) -> Result<Vec<crate::breakpoints::Breakpoint>, String> {
    let bps = crate::breakpoints::remove_breakpoint(&rules_dir(&app)?, &id)?;
    *state.breakpoints.write().unwrap() = bps.clone();
    Ok(bps)
}

#[tauri::command]
pub fn set_intercept(enabled: bool, state: State<'_, AppState>) {
    *state.intercept.write().unwrap() = enabled;
}

#[tauri::command]
pub fn get_intercept(state: State<'_, AppState>) -> bool {
    *state.intercept.read().unwrap()
}

#[tauri::command]
pub fn get_breakpoint_settings(app: AppHandle) -> Result<crate::breakpoints::BreakpointSettings, String> {
    Ok(crate::breakpoints::load_settings(&rules_dir(&app)?))
}

#[tauri::command]
pub fn set_breakpoint_settings(
    app: AppHandle,
    settings: crate::breakpoints::BreakpointSettings,
    state: State<'_, AppState>,
) -> Result<(), String> {
    crate::breakpoints::save_settings(&rules_dir(&app)?, &settings).map_err(|e| e.to_string())?;
    *state.breakpoint_timeout.write().unwrap() = settings.timeout_secs;
    *state.pause_others.write().unwrap() = settings.pause_others;
    Ok(())
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditedPayload {
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub status: Option<u16>,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    #[serde(default)]
    pub body: String,
    /// Raw body as base64 (an uploaded file); overrides `body` when present.
    #[serde(default)]
    pub body_base64: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

/// Общее ядро resolve: используется Tauri-командой и MCP-тулом.
pub fn resolve_breakpoint_core(
    pending: &crate::proxy::BreakpointRegistry,
    id: u64,
    phase: &str,
    action: &str,
    edited: EditedPayload,
) -> Result<(), String> {
    use base64::Engine;
    use crate::proxy::{BpPhase, Resolution};
    let bp_phase = match phase {
        "request" => BpPhase::Request,
        "response" => BpPhase::Response,
        _ => return Err("bad phase".into()),
    };
    // Decode an uploaded file body (base64) into raw bytes, if present.
    let body_bytes = match edited.body_base64 {
        Some(b64) => Some(
            base64::engine::general_purpose::STANDARD
                .decode(b64.as_bytes())
                .map_err(|e| format!("bad base64 body: {e}"))?,
        ),
        None => None,
    };
    let resolution = match action {
        "execute" => Resolution::Execute {
            method: edited.method,
            path: edited.path,
            status: edited.status,
            headers: edited.headers,
            body: edited.body,
            body_bytes,
        },
        "abort" => Resolution::Abort(edited.reason.unwrap_or_else(|| "aborted".into())),
        "respond" => Resolution::Respond {
            status: edited.status.unwrap_or(200),
            headers: edited.headers,
            body: edited.body,
            body_bytes,
        },
        _ => return Err("bad action".into()),
    };
    let tx = pending.lock().unwrap().remove(&(id, bp_phase));
    match tx {
        Some(tx) => {
            let _ = tx.send(resolution);
            Ok(())
        }
        None => Err("no pending breakpoint".into()),
    }
}

#[tauri::command]
pub fn resolve_breakpoint(
    id: u64,
    phase: String,
    action: String,
    edited: EditedPayload,
    state: State<'_, AppState>,
) -> Result<(), String> {
    resolve_breakpoint_core(&state.pending_breakpoints, id, &phase, &action, edited)
}

#[tauri::command]
pub fn get_library(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    let lib = rules::load_library(&rules_dir(&app)?).map_err(|e| e.to_string())?;
    *state.library.write().unwrap() = lib.clone();
    Ok(lib)
}

#[tauri::command]
pub fn save_library(app: AppHandle, source: String, state: State<'_, AppState>) -> Result<(), String> {
    rules::save_library(&rules_dir(&app)?, &source).map_err(|e| e.to_string())?;
    *state.library.write().unwrap() = source;
    Ok(())
}

// ── User templates & snippets ──

#[tauri::command]
pub fn get_snippets(app: AppHandle) -> Result<crate::snippets::SnippetsFile, String> {
    crate::snippets::load_snippets(&rules_dir(&app)?).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_snippets(
    app: AppHandle,
    file: crate::snippets::SnippetsFile,
) -> Result<(), String> {
    crate::snippets::save_snippets(&rules_dir(&app)?, &file).map_err(|e| e.to_string())
}

// ── Проекты ──

#[tauri::command]
pub fn list_projects(app: AppHandle) -> Result<ProjectsFile, String> {
    projects::load_projects(&data_dir(&app)?).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_project(
    app: AppHandle,
    project: Project,
    state: State<'_, AppState>,
) -> Result<ProjectsFile, String> {
    let file = projects::upsert_project(&data_dir(&app)?, project.clone())?;
    // если правим активный проект — обновить общую ячейку
    let mut active = state.active_project.write().unwrap();
    if active.as_ref().map(|p| &p.id) == Some(&project.id) {
        *active = Some(project);
    }
    Ok(file)
}

#[tauri::command]
pub fn delete_project(
    app: AppHandle,
    id: String,
    state: State<'_, AppState>,
) -> Result<ProjectsFile, String> {
    let file = projects::remove_project(&data_dir(&app)?, &id)?;
    if file.active_id.is_none() {
        let mut active = state.active_project.write().unwrap();
        if active.as_ref().map(|p| p.id.as_str()) == Some(id.as_str()) {
            *active = None;
        }
    }
    Ok(file)
}

#[tauri::command]
pub fn set_active_project(
    app: AppHandle,
    id: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let resolved = projects::set_active(&data_dir(&app)?, id)?;
    *state.active_project.write().unwrap() = resolved;
    Ok(())
}

#[tauri::command]
pub fn get_active_project(state: State<'_, AppState>) -> Option<Project> {
    state.active_project.read().unwrap().clone()
}

// ── Persistent flow DB (analytics) ──

#[tauri::command]
pub fn query_flows(
    filter: FlowQuery,
    limit: u32,
    offset: u32,
    state: State<'_, AppState>,
) -> Result<Vec<FlowRow>, String> {
    let db = state.db()?.reader().map_err(|e| e.to_string())?;
    db.query(&filter, limit, offset).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn flow_count(filter: FlowQuery, state: State<'_, AppState>) -> Result<u64, String> {
    let db = state.db()?.reader().map_err(|e| e.to_string())?;
    db.count(&filter).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn aggregate_flows(
    filter: FlowQuery,
    group_by: String,
    bucket: u64,
    limit: u32,
    state: State<'_, AppState>,
) -> Result<Vec<AggBucket>, String> {
    let db = state.db()?.reader().map_err(|e| e.to_string())?;
    db.aggregate(&filter, &group_by, bucket, limit)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_report(report: Report, state: State<'_, AppState>) -> Result<(), String> {
    let db = state.db()?.reader().map_err(|e| e.to_string())?;
    db.save_report(&report).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_reports(state: State<'_, AppState>) -> Result<Vec<Report>, String> {
    let db = state.db()?.reader().map_err(|e| e.to_string())?;
    db.list_reports().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_report(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let db = state.db()?.reader().map_err(|e| e.to_string())?;
    db.delete_report(&id).map_err(|e| e.to_string())
}

// ── HTTP client (one-shot send) ──

#[tauri::command]
pub async fn send_request(
    request: crate::httpsend::SendRequest,
    via_proxy: bool,
) -> Result<crate::httpsend::SendResponse, String> {
    tokio::task::spawn_blocking(move || crate::httpsend::send_http(&request, via_proxy))
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use crate::proxy::{BpPhase, BreakpointRegistry, Resolution};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn resolve_sends_into_registry() {
        let pending: BreakpointRegistry = Arc::new(Mutex::new(HashMap::new()));
        let (tx, rx) = oneshot::channel::<Resolution>();
        pending.lock().unwrap().insert((7, BpPhase::Request), tx);

        let taken = pending.lock().unwrap().remove(&(7, BpPhase::Request));
        assert!(taken.is_some());
        let _ = taken.unwrap().send(Resolution::Abort("x".into()));

        match rx.await.unwrap() {
            Resolution::Abort(r) => assert_eq!(r, "x"),
            _ => panic!("wrong resolution"),
        }
    }
}
