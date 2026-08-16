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
