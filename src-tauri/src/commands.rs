use std::net::SocketAddr;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::ca::load_or_create_ca;
use crate::model::Flow;
use crate::net::lan_ip;
use crate::proxy::{self, ProxyHandle};
use crate::store::FlowStore;

pub struct AppState {
    pub store: FlowStore,
    pub proxy: Mutex<Option<ProxyHandle>>,
}

impl AppState {
    pub fn new() -> Self {
        AppState { store: FlowStore::new(5000), proxy: Mutex::new(None) }
    }
}

fn ca_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("ca"))
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
    let handle = proxy::start(addr, state.store.clone(), emit, ca_dir(&app)?)
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
