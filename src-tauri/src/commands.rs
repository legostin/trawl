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
}

impl AppState {
    pub fn new() -> Self {
        AppState {
            store: FlowStore::new(5000),
            proxy: Mutex::new(None),
            rules: Arc::new(RwLock::new(Vec::new())),
            library: Arc::new(RwLock::new(String::new())),
            active_project: Arc::new(RwLock::new(None)),
            scripts: crate::scripting::spawn_engine(std::time::Duration::from_secs(1)),
            db: OnceLock::new(),
            breakpoints: Arc::new(RwLock::new(Vec::new())),
            intercept: Arc::new(RwLock::new(true)),
            pending_breakpoints: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }

    fn db(&self) -> Result<&DbHandle, String> {
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

fn data_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

fn ca_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("ca"))
}

fn rules_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
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
    // Подтянуть актуальные правила, библиотеку и активный проект перед стартом.
    let rdir = rules_dir(&app)?;
    let loaded_rules = rules::load_rules(&rdir).map_err(|e| e.to_string())?;
    let loaded_library = rules::load_library(&rdir).map_err(|e| e.to_string())?;
    let loaded_bps = crate::breakpoints::load_breakpoints(&rdir).map_err(|e| e.to_string())?;
    *state.rules.write().unwrap() = loaded_rules;
    *state.library.write().unwrap() = loaded_library;
    *state.breakpoints.write().unwrap() = loaded_bps;
    let pfile = projects::load_projects(&data_dir(&app)?).map_err(|e| e.to_string())?;
    *state.active_project.write().unwrap() = pfile
        .active_id
        .and_then(|i| pfile.projects.into_iter().find(|p| p.id == i));

    let handle = proxy::start(
        addr,
        state.store.clone(),
        emit,
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
    let dir = rules_dir(&app)?;
    let mut rules = rules::load_rules(&dir).map_err(|e| e.to_string())?;
    if let Some(existing) = rules.iter_mut().find(|r| r.id == rule.id) {
        *existing = rule;
    } else {
        rules.push(rule);
    }
    rules::save_rules(&dir, &rules).map_err(|e| e.to_string())?;
    *state.rules.write().unwrap() = rules.clone();
    Ok(rules)
}

#[tauri::command]
pub fn delete_rule(app: AppHandle, id: String, state: State<'_, AppState>) -> Result<Vec<Rule>, String> {
    let dir = rules_dir(&app)?;
    let mut rules = rules::load_rules(&dir).map_err(|e| e.to_string())?;
    rules.retain(|r| r.id != id);
    rules::save_rules(&dir, &rules).map_err(|e| e.to_string())?;
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
    let dir = rules_dir(&app)?;
    let mut bps = crate::breakpoints::load_breakpoints(&dir).map_err(|e| e.to_string())?;
    if let Some(existing) = bps.iter_mut().find(|b| b.id == breakpoint.id) {
        *existing = breakpoint;
    } else {
        bps.push(breakpoint);
    }
    crate::breakpoints::save_breakpoints(&dir, &bps).map_err(|e| e.to_string())?;
    *state.breakpoints.write().unwrap() = bps.clone();
    Ok(bps)
}

#[tauri::command]
pub fn delete_breakpoint(
    app: AppHandle,
    id: String,
    state: State<'_, AppState>,
) -> Result<Vec<crate::breakpoints::Breakpoint>, String> {
    let dir = rules_dir(&app)?;
    let mut bps = crate::breakpoints::load_breakpoints(&dir).map_err(|e| e.to_string())?;
    bps.retain(|b| b.id != id);
    crate::breakpoints::save_breakpoints(&dir, &bps).map_err(|e| e.to_string())?;
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
    #[serde(default)]
    pub reason: Option<String>,
}

#[tauri::command]
pub fn resolve_breakpoint(
    id: u64,
    phase: String,
    action: String,
    edited: EditedPayload,
    state: State<'_, AppState>,
) -> Result<(), String> {
    use crate::proxy::{BpPhase, Resolution};
    let bp_phase = match phase.as_str() {
        "request" => BpPhase::Request,
        "response" => BpPhase::Response,
        _ => return Err("bad phase".into()),
    };
    let resolution = match action.as_str() {
        "execute" => Resolution::Execute {
            method: edited.method,
            path: edited.path,
            status: edited.status,
            headers: edited.headers,
            body: edited.body,
        },
        "abort" => Resolution::Abort(edited.reason.unwrap_or_else(|| "aborted".into())),
        "respond" => Resolution::Respond {
            status: edited.status.unwrap_or(200),
            headers: edited.headers,
            body: edited.body,
        },
        _ => return Err("bad action".into()),
    };
    let tx = state.pending_breakpoints.lock().unwrap().remove(&(id, bp_phase));
    match tx {
        Some(tx) => {
            let _ = tx.send(resolution);
            Ok(())
        }
        None => Err("no pending breakpoint".into()),
    }
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
    let dir = data_dir(&app)?;
    let mut file = projects::load_projects(&dir).map_err(|e| e.to_string())?;
    if let Some(existing) = file.projects.iter_mut().find(|p| p.id == project.id) {
        *existing = project.clone();
    } else {
        file.projects.push(project.clone());
    }
    projects::save_projects(&dir, &file).map_err(|e| e.to_string())?;
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
    let dir = data_dir(&app)?;
    let mut file = projects::load_projects(&dir).map_err(|e| e.to_string())?;
    file.projects.retain(|p| p.id != id);
    if file.active_id.as_deref() == Some(&id) {
        file.active_id = None;
        *state.active_project.write().unwrap() = None;
    }
    projects::save_projects(&dir, &file).map_err(|e| e.to_string())?;
    Ok(file)
}

#[tauri::command]
pub fn set_active_project(
    app: AppHandle,
    id: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let dir = data_dir(&app)?;
    let mut file = projects::load_projects(&dir).map_err(|e| e.to_string())?;
    file.active_id = id.clone();
    let resolved = id.and_then(|i| file.projects.iter().find(|p| p.id == i).cloned());
    projects::save_projects(&dir, &file).map_err(|e| e.to_string())?;
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
