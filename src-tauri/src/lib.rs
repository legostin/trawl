mod breakpoints;
mod ca;
mod commands;
mod db;
mod httpsend;
mod mcp;
mod model;
mod net;
mod plugins;
mod projects;
mod proxy;
mod rules;
mod scripting;
mod secrets;
mod snippets;
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
        .manage(mcp::McpState::new())
        .setup(|app| {
            use tauri::Manager;
            let state = app.state::<AppState>();
            if let Err(e) = commands::init_db(app.handle(), &state) {
                eprintln!("failed to initialize flow DB: {e}");
            }
            match commands::data_dir(app.handle()) {
                Ok(dir) => {
                    let cfg = mcp::load_config(&dir);
                    let handle = app.handle().clone();
                    tauri::async_runtime::spawn(async move {
                        mcp::apply_config(&handle, &cfg).await;
                    });
                }
                Err(e) => eprintln!("mcp: no data dir: {e}"),
            }
            Ok(())
        })
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
            commands::list_breakpoints,
            commands::save_breakpoint,
            commands::delete_breakpoint,
            commands::set_intercept,
            commands::get_intercept,
            commands::get_breakpoint_settings,
            commands::set_breakpoint_settings,
            commands::resolve_breakpoint,
            commands::get_library,
            commands::save_library,
            commands::get_snippets,
            commands::save_snippets,
            commands::list_projects,
            commands::save_project,
            commands::delete_project,
            commands::set_active_project,
            commands::get_active_project,
            commands::query_flows,
            commands::flow_count,
            commands::aggregate_flows,
            commands::save_report,
            commands::list_reports,
            commands::delete_report,
            commands::send_request,
            plugins::fetch_plugin_catalog,
            plugins::fetch_plugin_manifest,
            plugins::install_plugin,
            plugins::list_plugins,
            plugins::set_plugin_enabled,
            plugins::remove_plugin,
            plugins::read_plugin_bundle,
            plugins::plugin_storage_get,
            plugins::plugin_storage_set,
            plugins::git_host_token_set,
            plugins::git_host_token_has,
            plugins::git_host_token_get,
            secrets::secrets_list,
            secrets::secret_get,
            secrets::secret_set,
            secrets::secret_delete,
            setup_actions::reveal_ca_cert,
            setup_actions::trust_ca_macos,
            setup_actions::trust_ca_command,
            setup_actions::set_system_proxy,
            setup_actions::system_proxy_enabled,
            setup_actions::install_ca_ios_simulator,
            setup_actions::ios_simulator_booted,
            setup_actions::launch_chrome_proxy,
            mcp::mcp_get_config,
            mcp::mcp_set_config,
            mcp::mcp_regen_token,
            mcp::mcp_server_status,
            mcp::plugin_bridge::mcp_register_tool,
            mcp::plugin_bridge::mcp_unregister_tool,
            mcp::plugin_bridge::mcp_clear_plugin_tools,
            mcp::plugin_bridge::mcp_tool_result,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
