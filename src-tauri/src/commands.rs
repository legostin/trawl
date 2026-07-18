use std::net::SocketAddr;
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, State};

use crate::model::Flow;
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
    let handle = proxy::start(addr, state.store.clone(), emit)
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
