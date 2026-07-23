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
        env_list_object(&self.env)
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

/// env-список как JSON-объект (пустые ключи пропускаются).
pub fn env_list_object(env: &[EnvVar]) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    for e in env {
        if !e.key.is_empty() {
            m.insert(e.key.clone(), serde_json::Value::String(e.value.clone()));
        }
    }
    serde_json::Value::Object(m)
}

/// Эффективный env: global, поверх — env проекта (при совпадении ключа побеждает проект).
pub fn merged_env_object(global: &[EnvVar], project: Option<&Project>) -> serde_json::Value {
    let mut m = match env_list_object(global) {
        serde_json::Value::Object(m) => m,
        _ => serde_json::Map::new(),
    };
    if let Some(p) = project {
        if let serde_json::Value::Object(pm) = p.env_object() {
            for (k, v) in pm {
                m.insert(k, v);
            }
        }
    }
    serde_json::Value::Object(m)
}

/// Новый env проекта после записи скриптом (`returned` — изменённый merged-объект):
/// остаются ключи, которые уже были в проекте ИЛИ отличаются от глобального значения.
/// Нетронутые глобальные ключи в проект не копируются.
pub fn split_env_writeback(
    returned: &serde_json::Value,
    project_env: &[EnvVar],
    global: &[EnvVar],
) -> Vec<EnvVar> {
    let gobj = env_list_object(global);
    let project_keys: std::collections::HashSet<&str> =
        project_env.iter().map(|e| e.key.as_str()).collect();
    match returned.as_object() {
        Some(obj) => obj
            .iter()
            .filter(|(k, v)| project_keys.contains(k.as_str()) || gobj.get(k.as_str()) != Some(*v))
            .map(|(k, v)| EnvVar {
                key: k.clone(),
                value: match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                },
            })
            .collect(),
        None => project_env.to_vec(),
    }
}

/// Обновляет глобальный env на диске.
pub fn update_global_env(dir: &Path, env: Vec<EnvVar>) -> Result<()> {
    let mut file = load_projects(dir)?;
    file.global_env = env;
    save_projects(dir, &file)
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
    /// Глобальные переменные (вне проектов); проектные перекрывают их при merge.
    #[serde(default)]
    pub global_env: Vec<EnvVar>,
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

pub fn upsert_project(dir: &Path, project: Project) -> Result<ProjectsFile, String> {
    let mut file = load_projects(dir).map_err(|e| e.to_string())?;
    if let Some(existing) = file.projects.iter_mut().find(|p| p.id == project.id) {
        *existing = project;
    } else {
        file.projects.push(project);
    }
    save_projects(dir, &file).map_err(|e| e.to_string())?;
    Ok(file)
}

pub fn remove_project(dir: &Path, id: &str) -> Result<ProjectsFile, String> {
    let mut file = load_projects(dir).map_err(|e| e.to_string())?;
    file.projects.retain(|p| p.id != id);
    if file.active_id.as_deref() == Some(id) {
        file.active_id = None;
    }
    save_projects(dir, &file).map_err(|e| e.to_string())?;
    Ok(file)
}

/// Сохраняет active_id и возвращает резолвнутый активный проект.
pub fn set_active(dir: &Path, id: Option<String>) -> Result<Option<Project>, String> {
    let mut file = load_projects(dir).map_err(|e| e.to_string())?;
    file.active_id = id.clone();
    let resolved = id.and_then(|i| file.projects.iter().find(|p| p.id == i).cloned());
    save_projects(dir, &file).map_err(|e| e.to_string())?;
    Ok(resolved)
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
    fn merged_env_project_wins() {
        let global = vec![
            EnvVar { key: "HOST".into(), value: "global.example.com".into() },
            EnvVar { key: "TOKEN".into(), value: "g-token".into() },
        ];
        let mut p = proj(&[], &[]);
        p.env = vec![EnvVar { key: "TOKEN".into(), value: "p-token".into() }];
        let m = merged_env_object(&global, Some(&p));
        assert_eq!(m["HOST"], "global.example.com", "глобальный ключ виден");
        assert_eq!(m["TOKEN"], "p-token", "проект побеждает при совпадении");
        let m = merged_env_object(&global, None);
        assert_eq!(m["TOKEN"], "g-token", "без проекта — только глобальные");
    }

    #[test]
    fn projects_file_without_global_env_loads() {
        let json = r#"{ "projects": [], "activeId": null }"#;
        let f: ProjectsFile = serde_json::from_str(json).unwrap();
        assert!(f.global_env.is_empty(), "старый файл без globalEnv читается");
    }

    #[test]
    fn update_global_env_persists() {
        let dir = tempfile::tempdir().unwrap();
        update_global_env(dir.path(), vec![EnvVar { key: "G".into(), value: "1".into() }]).unwrap();
        let back = load_projects(dir.path()).unwrap();
        assert_eq!(back.global_env[0].key, "G");
        assert_eq!(back.global_env[0].value, "1");
    }

    #[test]
    fn writeback_untouched_global_not_copied() {
        let global = vec![EnvVar { key: "HOST".into(), value: "g".into() }];
        let project = vec![EnvVar { key: "TOKEN".into(), value: "old".into() }];
        // скрипт ничего не менял — merged вернулся как есть
        let returned = serde_json::json!({ "HOST": "g", "TOKEN": "old" });
        let out = split_env_writeback(&returned, &project, &global);
        assert_eq!(out.len(), 1, "глобальный ключ не утёк в проект");
        assert_eq!(out[0].key, "TOKEN");
    }

    #[test]
    fn writeback_modified_global_becomes_override() {
        let global = vec![EnvVar { key: "HOST".into(), value: "g".into() }];
        let returned = serde_json::json!({ "HOST": "changed" });
        let out = split_env_writeback(&returned, &[], &global);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key, "HOST");
        assert_eq!(out[0].value, "changed", "изменённый глобальный — проектное перекрытие");
    }

    #[test]
    fn writeback_new_key_goes_to_project() {
        let returned = serde_json::json!({ "NEW": "v" });
        let out = split_env_writeback(&returned, &[], &[]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key, "NEW");
    }

    #[test]
    fn writeback_deleted_project_key_removed() {
        let project = vec![EnvVar { key: "GONE".into(), value: "x".into() }];
        let returned = serde_json::json!({});
        let out = split_env_writeback(&returned, &project, &[]);
        assert!(out.is_empty(), "удалённый скриптом ключ исчезает из проекта");
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
        let file = ProjectsFile { projects: vec![proj(&["x"], &[])], active_id: Some("p1".into()), global_env: vec![] };
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
            global_env: vec![],
        };
        save_projects(&tmp, &file).unwrap();
        let back = load_projects(&tmp).unwrap();
        assert_eq!(back.projects.len(), 1);
        assert_eq!(back.active_id.as_deref(), Some("p1"));
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn upsert_remove_and_set_active_project() {
        let dir = tempfile::tempdir().unwrap();
        let p = Project {
            id: "p1".into(), name: "P".into(),
            include_hosts: vec!["example.com".into()],
            exclude_hosts: vec![], env: vec![],
        };
        let file = upsert_project(dir.path(), p.clone()).unwrap();
        assert_eq!(file.projects.len(), 1);
        let active = set_active(dir.path(), Some("p1".into())).unwrap();
        assert_eq!(active.unwrap().id, "p1");
        let file = remove_project(dir.path(), "p1").unwrap();
        assert!(file.projects.is_empty());
        // remove активного снимает active_id
        assert!(file.active_id.is_none());
    }
}
