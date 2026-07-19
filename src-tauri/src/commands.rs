use std::net::SocketAddr;
use std::sync::{Arc, Mutex, RwLock};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::ca::load_or_create_ca;
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
        }
    }
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
    // Подтянуть актуальные правила и библиотеку в общие ячейки перед стартом.
    let rdir = rules_dir(&app)?;
    let loaded_rules = rules::load_rules(&rdir).map_err(|e| e.to_string())?;
    let loaded_library = rules::load_library(&rdir).map_err(|e| e.to_string())?;
    *state.rules.write().unwrap() = loaded_rules;
    *state.library.write().unwrap() = loaded_library;

    let handle = proxy::start(
        addr,
        state.store.clone(),
        emit,
        ca_dir(&app)?,
        state.scripts.clone(),
        state.rules.clone(),
        state.library.clone(),
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
        port: 8888,
        cert_host: "http-catch".into(),
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
