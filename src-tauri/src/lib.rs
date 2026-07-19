mod ca;
mod commands;
mod model;
mod net;
mod projects;
mod proxy;
mod rules;
mod scripting;
mod store;

use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::start_proxy,
            commands::stop_proxy,
            commands::get_flows,
            commands::get_setup_info,
            commands::get_ca_pem,
            commands::ca_cert_path,
            commands::list_rules,
            commands::save_rule,
            commands::delete_rule,
            commands::get_library,
            commands::save_library,
            commands::list_projects,
            commands::save_project,
            commands::delete_project,
            commands::set_active_project,
            commands::get_active_project,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
