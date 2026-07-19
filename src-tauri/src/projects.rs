use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::rules::glob_to_regex;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvVar {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    /// Хосты для отслеживания (glob или голый домен → домен + поддомены).
    pub include_hosts: Vec<String>,
    /// Хосты-исключения (приоритетнее include).
    pub exclude_hosts: Vec<String>,
    /// Переменные окружения проекта, доступные в скриптах как env.KEY.
    #[serde(default)]
    pub env: Vec<EnvVar>,
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

    /// env как JSON-объект для инъекции в скрипт (`env.KEY`).
    pub fn env_object(&self) -> serde_json::Value {
        let mut m = serde_json::Map::new();
        for e in &self.env {
            if !e.key.is_empty() {
                m.insert(e.key.clone(), serde_json::Value::String(e.value.clone()));
            }
        }
        serde_json::Value::Object(m)
    }
}

/// Преобразует JSON-объект env обратно в упорядоченный Vec<EnvVar>.
pub fn env_from_object(v: &serde_json::Value) -> Vec<EnvVar> {
    match v.as_object() {
        Some(obj) => obj
            .iter()
            .map(|(k, val)| EnvVar {
                key: k.clone(),
                value: match val {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                },
            })
            .collect(),
        None => vec![],
    }
}

/// Обновляет env указанного проекта на диске.
pub fn update_project_env(dir: &Path, project_id: &str, env: Vec<EnvVar>) -> Result<()> {
    let mut file = load_projects(dir)?;
    if let Some(p) = file.projects.iter_mut().find(|p| p.id == project_id) {
        p.env = env;
        save_projects(dir, &file)?;
    }
    Ok(())
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
            env: vec![],
        }
    }

    #[test]
    fn env_object_and_back() {
        let mut p = proj(&[], &[]);
        p.env = vec![
            EnvVar { key: "TOKEN".into(), value: "abc".into() },
            EnvVar { key: "".into(), value: "skip".into() },
        ];
        let obj = p.env_object();
        assert_eq!(obj["TOKEN"], "abc");
        assert!(obj.get("").is_none(), "пустой ключ пропускается");

        let back = env_from_object(&serde_json::json!({ "A": "1", "B": "2" }));
        assert_eq!(back.len(), 2);
    }

    #[test]
    fn update_env_persists() {
        let tmp = std::env::temp_dir().join(format!("httpcatch-env-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let file = ProjectsFile { projects: vec![proj(&["x"], &[])], active_id: Some("p1".into()) };
        save_projects(&tmp, &file).unwrap();
        update_project_env(&tmp, "p1", vec![EnvVar { key: "K".into(), value: "V".into() }]).unwrap();
        let back = load_projects(&tmp).unwrap();
        assert_eq!(back.projects[0].env[0].value, "V");
        std::fs::remove_dir_all(&tmp).unwrap();
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
