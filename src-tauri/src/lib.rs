mod ca;
mod commands;
mod model;
mod net;
mod projects;
mod proxy;
mod rules;
mod scripting;
mod setup_actions;
mod store;

use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
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
            setup_actions::reveal_ca_cert,
            setup_actions::trust_ca_macos,
            setup_actions::trust_ca_command,
            setup_actions::set_system_proxy,
            setup_actions::system_proxy_enabled,
            setup_actions::install_ca_ios_simulator,
            setup_actions::ios_simulator_booted,
            setup_actions::launch_chrome_proxy,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
