use serde::Serialize;

/// Which harnesses this machine can actually run, and where they are.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessAvailability {
    pub id: &'static str,
    pub label: &'static str,
    /// Absolute path when installed; `None` means the panel should say so
    /// before the user types a message rather than after.
    pub path: Option<String>,
}

pub const HARNESSES: [(&str, &str); 2] = [("claude", "Claude Code"), ("codex", "Codex CLI")];

/// Looks a command up the way a shell would: first executable match wins.
/// Entries that do not exist are skipped rather than treated as an error —
/// a stale PATH entry is ordinary, not a failure.
pub fn find_on_path(name: &str, path: &str) -> Option<String> {
    path.split(':')
        .filter(|dir| !dir.is_empty())
        .map(|dir| std::path::Path::new(dir).join(name))
        .find(|candidate| is_executable(candidate))
        .map(|p| p.to_string_lossy().to_string())
}

fn is_executable(p: &std::path::Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(p)
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        p.is_file()
    }
}

/// The PATH a terminal would have, since a GUI app does not inherit one.
fn search_path() -> String {
    crate::childproc::login_path()
        .or_else(|| std::env::var("PATH").ok())
        .unwrap_or_default()
}

#[tauri::command]
pub fn agent_harnesses() -> Vec<HarnessAvailability> {
    let path = search_path();
    HARNESSES
        .iter()
        .map(|(id, label)| HarnessAvailability {
            id,
            label,
            path: find_on_path(id, &path),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn tmpdir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("trawl-harness-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn put(dir: &std::path::Path, name: &str, executable: bool) {
        let p = dir.join(name);
        std::fs::write(&p, "#!/bin/sh\n").unwrap();
        let mode = if executable { 0o755 } else { 0o644 };
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(mode)).unwrap();
    }

    #[test]
    fn an_installed_harness_is_found_on_the_path() {
        let dir = tmpdir("found");
        put(&dir, "claude", true);
        let path = format!("/nowhere:{}", dir.display());
        assert_eq!(
            find_on_path("claude", &path),
            Some(dir.join("claude").to_string_lossy().to_string())
        );
    }

    #[test]
    fn a_missing_harness_is_reported_as_absent() {
        let dir = tmpdir("missing");
        assert_eq!(find_on_path("codex", &dir.display().to_string()), None);
    }

    #[test]
    fn a_file_without_the_executable_bit_does_not_count() {
        // A stray file of the right name is not a harness, and treating it as
        // one would swap "install it" for a spawn failure later.
        let dir = tmpdir("notexec");
        put(&dir, "codex", false);
        assert_eq!(find_on_path("codex", &dir.display().to_string()), None);
    }

    #[test]
    fn a_path_entry_that_does_not_exist_is_skipped_rather_than_fatal() {
        let dir = tmpdir("skip");
        put(&dir, "claude", true);
        let path = format!("/definitely/not/here::{}", dir.display());
        assert!(find_on_path("claude", &path).is_some());
    }
}
