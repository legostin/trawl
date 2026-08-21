mod agent;
mod artifacts;
mod atomicfile;
mod breakpoints;
mod ca;
mod childproc;
mod commands;
mod db;
pub mod dryrun;
pub mod hashing;
mod httpsend;
pub mod jsonpath;
mod mcp;
mod model;
mod net;
mod plugins;
mod portable;
mod projects;
mod proxy;
mod rules;
mod scripting;
pub mod script_state;
mod secrets;
mod snippets;
mod setup_actions;
mod store;

use commands::AppState;

/// How many descriptors the app asks for. A proxy holding a few hundred live
/// sockets is ordinary; 8192 leaves room for that plus the database, the
/// config files and whatever a plugin opens, and stays well under macOS's
/// per-process ceiling (`kern.maxfilesperproc`, 184320 by default).
const WANTED_FILES: u64 = 8192;

/// Raise the descriptor limit at startup.
///
/// launchd hands a GUI app a soft limit of **256**, while a terminal gives it
/// a million. Trawl's proxy alone holds a couple of hundred sockets, so under
/// launchd the very next open — the MCP config, the database, a plugin's
/// file — fails with "Too many open files". The hard limit is unlimited, so
/// this is ours to raise; it never lowers a limit that is already generous.
#[cfg(unix)]
fn raise_file_limit() {
    unsafe {
        let mut lim: libc::rlimit = std::mem::zeroed();
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim) != 0 {
            return;
        }
        let hard = lim.rlim_max;
        let target = if hard == libc::RLIM_INFINITY {
            WANTED_FILES as libc::rlim_t
        } else {
            std::cmp::min(WANTED_FILES as libc::rlim_t, hard)
        };
        if lim.rlim_cur >= target {
            return;
        }
        lim.rlim_cur = target;
        let _ = libc::setrlimit(libc::RLIMIT_NOFILE, &lim);
    }
}

#[cfg(unix)]
fn file_limit() -> u64 {
    unsafe {
        let mut lim: libc::rlimit = std::mem::zeroed();
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim) != 0 {
            return 0;
        }
        lim.rlim_cur as u64
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(unix)]
    raise_file_limit();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new())
        .manage(mcp::McpState::new())
        .manage(childproc::ProcState::new())
        .manage(agent::AgentState::new())
        .setup(|app| {
            use tauri::Manager;

            // A signal (a terminal Ctrl-C, a `kill`, a supervisor stopping the
            // app) skips Tauri's exit event, and plugin-started programs would
            // outlive the app that owns them. Handled on a thread, not in a
            // signal handler, so taking the registry's lock is safe.
            {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
                    match signal_hook::iterator::Signals::new([SIGINT, SIGTERM, SIGHUP]) {
                        Ok(mut signals) => {
                            if signals.forever().next().is_some() {
                                childproc::kill_all(&handle.state::<childproc::ProcState>());
                                handle.exit(0);
                            }
                        }
                        Err(e) => eprintln!("cannot watch for signals: {e}"),
                    }
                });
            }
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
            plugins::report_plugin_load,
            portable::export_config,
            portable::import_config,
            artifacts::open_artifact,
            artifacts::reveal_artifact,
            agent::harness::agent_harnesses,
            agent::agent_send,
            agent::agent_interrupt,
            agent::agent_reset,
            agent::agent_status,
            commands::start_proxy,
            commands::stop_proxy,
            commands::get_flows,
            commands::get_setup_info,
            commands::get_ca_pem,
            commands::ca_cert_path,
            commands::list_rules,
            commands::save_rule,
            commands::delete_rule,
            commands::validate_jsonpath,
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
            commands::save_global_env,
            commands::query_flows,
            commands::flow_count,
            commands::aggregate_flows,
            commands::save_report,
            commands::list_reports,
            commands::delete_report,
            commands::send_request,
            commands::test_rule,
            commands::test_path,
            plugins::fetch_plugin_catalog,
            plugins::fetch_plugin_manifest,
            plugins::install_plugin,
            plugins::list_plugins,
            plugins::set_plugin_enabled,
            plugins::remove_plugin,
            plugins::read_plugin_bundle,
            childproc::plugin_spawn,
            childproc::plugin_kill_process,
            childproc::plugin_list_processes,
            childproc::plugin_kill_processes,
            plugins::plugin_storage_get,
            plugins::plugin_storage_set,
            plugins::git_host_token_set,
            plugins::git_host_token_has,
            plugins::git_host_token_get,
            plugins::git_hosts_list,
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
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // Plugin-started processes must not outlive the app.
            if matches!(event, tauri::RunEvent::Exit) {
                use tauri::Manager;
                childproc::kill_all(&app.state::<childproc::ProcState>());
            }
        });
}


#[cfg(all(test, unix))]
mod limit_tests {
    use super::*;

    #[test]
    fn the_descriptor_limit_is_raised_from_what_launchd_gives_a_gui_app() {
        // Reproduce the real condition rather than asserting against a
        // terminal's generous limit: launchd starts a GUI app at 256, which a
        // few hundred live proxy sockets exhaust on their own — that is the
        // "Too many open files" seen only when Trawl is opened from Finder.
        unsafe {
            let mut lim: libc::rlimit = std::mem::zeroed();
            assert_eq!(libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim), 0);
            let restore = lim;
            lim.rlim_cur = 256;
            assert_eq!(libc::setrlimit(libc::RLIMIT_NOFILE, &lim), 0);
            assert_eq!(file_limit(), 256, "the launchd condition did not take");

            raise_file_limit();
            let raised = file_limit();

            let _ = libc::setrlimit(libc::RLIMIT_NOFILE, &restore);
            assert!(raised >= WANTED_FILES, "soft limit stayed at {raised}");
        }
    }

    #[test]
    fn raising_is_idempotent_and_never_lowers() {
        raise_file_limit();
        let first = file_limit();
        raise_file_limit();
        assert_eq!(file_limit(), first);
    }
}
