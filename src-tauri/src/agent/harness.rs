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

/// Where the usual installers put things, checked only after the PATH.
const WELL_KNOWN: [&str; 5] = [
    ".local/bin",
    ".npm-global/bin",
    ".bun/bin",
    ".volta/bin",
    ".yarn/bin",
];

/// The PATH first, then the handful of places a harness is normally installed.
/// The login shell can fail to answer at all — a broken rc file, a minimal
/// container — and "install it" is the wrong thing to say to someone who
/// already did.
pub fn find_anywhere(name: &str, path: &str, home: Option<&std::path::Path>) -> Option<String> {
    if let Some(hit) = find_on_path(name, path) {
        return Some(hit);
    }
    let home: std::path::PathBuf = match home {
        Some(h) => h.to_path_buf(),
        None => std::env::var_os("HOME")?.into(),
    };
    WELL_KNOWN
        .iter()
        .map(|dir| home.join(dir).join(name))
        .find(|candidate| is_executable(candidate))
        .map(|p| p.to_string_lossy().to_string())
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
            path: find_anywhere(id, &path, None),
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
    fn a_harness_outside_the_path_is_still_found_in_a_well_known_place() {
        // The login shell can fail entirely (a broken rc file, a minimal
        // container). These are where npm, nvm and the official installers put
        // things, so looking there beats telling the user to install what they
        // already have.
        let home = tmpdir("wellknown");
        let bin = home.join(".local/bin");
        std::fs::create_dir_all(&bin).unwrap();
        put(&bin, "claude", true);
        assert_eq!(
            find_anywhere("claude", "/nowhere", Some(&home)),
            Some(bin.join("claude").to_string_lossy().to_string())
        );
    }

    #[test]
    fn the_path_still_wins_over_a_well_known_place() {
        let home = tmpdir("pathwins");
        let bin = home.join(".local/bin");
        std::fs::create_dir_all(&bin).unwrap();
        put(&bin, "codex", true);
        let on_path = tmpdir("pathwins-path");
        put(&on_path, "codex", true);
        assert_eq!(
            find_anywhere("codex", &on_path.display().to_string(), Some(&home)),
            Some(on_path.join("codex").to_string_lossy().to_string())
        );
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
