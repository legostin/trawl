//! Files the agent produces for the user — a CSV of failing requests, a report
//! to hand to someone else.
//!
//! This is the agent's only way to write a file, and it is confined to one
//! directory on purpose: everything else it can reach is Trawl's own state,
//! visible and undoable in the UI. A name that could climb out of that
//! directory is refused rather than sanitised, so a rejected write is obvious
//! instead of landing somewhere surprising.

use std::path::{Path, PathBuf};

pub fn artifacts_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("artifacts")
}

/// A file name that is safe as a single path component.
pub fn validate_artifact_name(name: &str) -> Result<(), String> {
    let bad = name.is_empty()
        || name.len() > 128
        || name == "."
        || name == ".."
        || name.starts_with('.')
        || name.contains('/')
        || name.contains('\\')
        || name.contains('\0');
    if bad {
        return Err(format!(
            "invalid artifact name {name:?}: one file name, no slashes, not starting with a dot"
        ));
    }
    Ok(())
}

/// Writes an artifact and returns its full path.
pub fn save_artifact(data_dir: &Path, name: &str, contents: &str) -> Result<PathBuf, String> {
    validate_artifact_name(name)?;
    let dir = artifacts_dir(data_dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(name);
    std::fs::write(&path, contents).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Resolves an artifact by name, refusing anything that is not actually inside
/// the artifacts directory.
pub fn artifact_path(data_dir: &Path, name: &str) -> Result<PathBuf, String> {
    validate_artifact_name(name)?;
    let path = artifacts_dir(data_dir).join(name);
    if !path.is_file() {
        return Err(format!("no artifact \"{name}\""));
    }
    Ok(path)
}

fn open_with(arg: Option<&str>, path: &Path) -> Result<(), String> {
    let mut cmd = std::process::Command::new("open");
    if let Some(a) = arg {
        cmd.arg(a);
    }
    cmd.arg(path);
    let out = cmd.output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

#[tauri::command]
pub fn open_artifact(app: tauri::AppHandle, name: String) -> Result<(), String> {
    let path = artifact_path(&crate::commands::data_dir(&app)?, &name)?;
    open_with(None, &path)
}

#[tauri::command]
pub fn reveal_artifact(app: tauri::AppHandle, name: String) -> Result<(), String> {
    let path = artifact_path(&crate::commands::data_dir(&app)?, &name)?;
    open_with(Some("-R"), &path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_that_climbs_out_of_the_directory_is_refused() {
        for bad in ["../escape.csv", "a/b.csv", "", ".", "..", ".hidden", "a\0b"] {
            assert!(validate_artifact_name(bad).is_err(), "{bad:?} must be refused");
        }
    }

    #[test]
    fn an_ordinary_file_name_is_accepted() {
        for ok in ["errors.csv", "report 2026.md", "flows-500.json"] {
            assert!(validate_artifact_name(ok).is_ok(), "{ok:?} must be accepted");
        }
    }

    #[test]
    fn saving_puts_the_file_where_the_link_will_look_for_it() {
        let tmp = tempfile::tempdir().unwrap();
        let path = save_artifact(tmp.path(), "errors.csv", "host,count\napi,3\n").unwrap();
        assert_eq!(path, artifacts_dir(tmp.path()).join("errors.csv"));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "host,count\napi,3\n"
        );
        assert_eq!(artifact_path(tmp.path(), "errors.csv").unwrap(), path);
    }

    #[test]
    fn resolving_something_that_was_never_written_says_so() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(artifact_path(tmp.path(), "nope.csv").is_err());
    }
}
