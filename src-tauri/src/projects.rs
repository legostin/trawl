use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::rules::glob_to_regex;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    /// Хосты для отслеживания (glob или голый домен → домен + поддомены).
    pub include_hosts: Vec<String>,
    /// Хосты-исключения (приоритетнее include).
    pub exclude_hosts: Vec<String>,
}

/// Матч хоста: голый домен ловит сам домен и поддомены; с `*` — как glob.
pub fn host_matches(entry: &str, host: &str) -> bool {
    let entry = entry.trim();
    if entry.is_empty() {
        return false;
    }
    if entry.contains('*') || entry.contains('?') {
        return glob_to_regex(entry).map(|re| re.is_match(host)).unwrap_or(false);
    }
    host == entry || host.ends_with(&format!(".{entry}"))
}

impl Project {
    /// Трекается ли хост этим проектом (include && !exclude).
    pub fn tracks(&self, host: &str) -> bool {
        let excluded = self.exclude_hosts.iter().any(|e| host_matches(e, host));
        if excluded {
            return false;
        }
        self.include_hosts.iter().any(|e| host_matches(e, host))
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectsFile {
    pub projects: Vec<Project>,
    pub active_id: Option<String>,
}

pub fn load_projects(dir: &Path) -> Result<ProjectsFile> {
    let path = dir.join("projects.json");
    if !path.exists() {
        return Ok(ProjectsFile::default());
    }
    let text = fs::read_to_string(&path).context("read projects.json")?;
    serde_json::from_str(&text).context("parse projects.json")
}

pub fn save_projects(dir: &Path, file: &ProjectsFile) -> Result<()> {
    fs::create_dir_all(dir).context("create projects dir")?;
    let text = serde_json::to_string_pretty(file).context("serialize projects")?;
    fs::write(dir.join("projects.json"), text).context("write projects.json")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proj(include: &[&str], exclude: &[&str]) -> Project {
        Project {
            id: "p1".into(),
            name: "test".into(),
            include_hosts: include.iter().map(|s| s.to_string()).collect(),
            exclude_hosts: exclude.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn bare_host_matches_domain_and_subdomains() {
        assert!(host_matches("example.com", "example.com"));
        assert!(host_matches("example.com", "api.example.com"));
        assert!(!host_matches("example.com", "notexample.com"));
        assert!(!host_matches("example.com", "example.org"));
    }

    #[test]
    fn wildcard_host_uses_glob() {
        assert!(host_matches("*.example.com", "api.example.com"));
        assert!(!host_matches("*.example.com", "example.com"));
    }

    #[test]
    fn tracks_include_and_exclude() {
        let p = proj(&["example.com"], &["static.example.com"]);
        assert!(p.tracks("api.example.com"));
        assert!(p.tracks("example.com"));
        assert!(!p.tracks("static.example.com")); // exclude приоритетнее
        assert!(!p.tracks("other.org"));
    }

    #[test]
    fn empty_include_tracks_nothing() {
        let p = proj(&[], &[]);
        assert!(!p.tracks("example.com"));
    }

    #[test]
    fn projects_roundtrip() {
        let tmp = std::env::temp_dir().join(format!("httpcatch-proj-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(load_projects(&tmp).unwrap().projects.is_empty());
        let file = ProjectsFile {
            projects: vec![proj(&["example.com"], &[])],
            active_id: Some("p1".into()),
        };
        save_projects(&tmp, &file).unwrap();
        let back = load_projects(&tmp).unwrap();
        assert_eq!(back.projects.len(), 1);
        assert_eq!(back.active_id.as_deref(), Some("p1"));
        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
