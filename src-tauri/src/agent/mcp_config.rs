use crate::mcp::core_tools::{changed_by, core_tools};

/// Tools that read and change nothing. Derived from the registry rather than
/// listed by hand, so a tool added later cannot quietly join the allowlist.
/// `send_request` is excluded separately: it changes no Trawl state but does
/// put real traffic on the wire.
pub fn read_only_tools() -> Vec<String> {
    core_tools()
        .iter()
        .map(|t| t.name.to_string())
        .filter(|name| changed_by(name).is_none() && name != "send_request")
        .collect()
}

pub fn mcp_config_json(port: u16, token: &str) -> String {
    serde_json::json!({
        "mcpServers": {
            "trawl": {
                "type": "http",
                "url": format!("http://127.0.0.1:{port}/mcp"),
                "headers": { "Authorization": format!("Bearer {token}") }
            }
        }
    })
    .to_string()
}

/// Writes the config next to the session's working directory and returns its
/// path. The token lives in a file only the user can read.
pub fn write_mcp_config_file(
    dir: &std::path::Path,
    port: u16,
    token: &str,
) -> std::io::Result<std::path::PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join("mcp.json");
    std::fs::write(&path, mcp_config_json(port, token))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_allowlist_tracks_the_tool_registry_rather_than_a_hand_written_copy() {
        let tools = read_only_tools();
        // Every writing tool is excluded, by asking the registry itself.
        for name in [
            "save_rule",
            "delete_rule",
            "set_active_project",
            "resolve_breakpoint",
        ] {
            assert!(!tools.iter().any(|t| t == name), "{name} must not be allowed");
        }
        for name in ["get_status", "query_flows", "get_flow", "list_rules"] {
            assert!(tools.iter().any(|t| t == name), "{name} must be allowed");
        }
    }

    #[test]
    fn the_exact_allowlist_is_pinned_so_a_new_tool_cannot_join_it_unnoticed() {
        // Deriving the list from `changed_by` protects against tools that
        // declare what they change. It does nothing about a mutating tool whose
        // author forgot to declare it — that one would silently land here.
        // Pinning the set turns that mistake into a failing test.
        let mut tools = read_only_tools();
        tools.sort();
        assert_eq!(
            tools,
            vec![
                "aggregate_flows",
                "flow_count",
                "get_flow",
                "get_scripting_reference",
                "get_status",
                "list_breakpoints",
                "list_paused",
                "list_projects",
                "list_rules",
                "query_flows",
                "test_rule",
            ]
        );
    }

    #[test]
    fn the_config_file_points_the_harness_at_our_server_with_its_token() {
        let json = mcp_config_json(9910, "tok-123");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["mcpServers"]["trawl"]["type"], "http");
        assert_eq!(v["mcpServers"]["trawl"]["url"], "http://127.0.0.1:9910/mcp");
        assert_eq!(
            v["mcpServers"]["trawl"]["headers"]["Authorization"],
            "Bearer tok-123"
        );
    }
}
