use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Request,
    Response,
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    /// glob-паттерн по строке `host+path`, напр. `api.example.com/*`, `*/v1/*`.
    pub pattern: String,
    pub phase: Phase,
    pub script: String,
}

impl Rule {
    /// Совпадает ли правило с целью `host+path` (без учёта фазы/enabled).
    pub fn matches_target(&self, target: &str) -> bool {
        match glob_to_regex(&self.pattern) {
            Ok(re) => re.is_match(target),
            Err(_) => false,
        }
    }

    /// Активно ли правило в данной фазе (учитывает enabled и совпадение цели).
    pub fn applies(&self, phase: Phase, target: &str) -> bool {
        self.enabled && self.runs_in(phase) && self.matches_target(target)
    }

    pub fn runs_in(&self, phase: Phase) -> bool {
        self.phase == Phase::Both || self.phase == phase
    }
}

/// Преобразует glob (`*`, `?`) в анкоренный regex.
pub fn glob_to_regex(glob: &str) -> Result<Regex> {
    let mut re = String::from("^");
    for ch in glob.chars() {
        match ch {
            '*' => re.push_str(".*"),
            '?' => re.push('.'),
            c if ".+()|[]{}^$\\".contains(c) => {
                re.push('\\');
                re.push(c);
            }
            c => re.push(c),
        }
    }
    re.push('$');
    Regex::new(&re).context("compile glob regex")
}

const DEFAULT_LIBRARY: &str =
    "// Библиотека переиспользуемых функций. Доступна во всех правилах.\n// Пример:\n// function withAuth(req, token) { req.headers['Authorization'] = 'Bearer ' + token; }\n";

pub fn load_rules(dir: &Path) -> Result<Vec<Rule>> {
    let path = dir.join("rules.json");
    if !path.exists() {
        return Ok(vec![]);
    }
    let text = fs::read_to_string(&path).context("read rules.json")?;
    let rules = serde_json::from_str(&text).context("parse rules.json")?;
    Ok(rules)
}

pub fn save_rules(dir: &Path, rules: &[Rule]) -> Result<()> {
    fs::create_dir_all(dir).context("create rules dir")?;
    let text = serde_json::to_string_pretty(rules).context("serialize rules")?;
    fs::write(dir.join("rules.json"), text).context("write rules.json")?;
    Ok(())
}

pub fn load_library(dir: &Path) -> Result<String> {
    let path = dir.join("library.js");
    if !path.exists() {
        return Ok(DEFAULT_LIBRARY.to_string());
    }
    fs::read_to_string(&path).context("read library.js")
}

pub fn save_library(dir: &Path, source: &str) -> Result<()> {
    fs::create_dir_all(dir).context("create rules dir")?;
    fs::write(dir.join("library.js"), source).context("write library.js")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(pattern: &str, phase: Phase) -> Rule {
        Rule {
            id: "1".into(),
            name: "t".into(),
            enabled: true,
            pattern: pattern.into(),
            phase,
            script: String::new(),
        }
    }

    #[test]
    fn host_prefix_glob_matches() {
        let r = rule("api.example.com/*", Phase::Request);
        assert!(r.matches_target("api.example.com/v1/users"));
        assert!(!r.matches_target("cdn.example.com/v1/users"));
    }

    #[test]
    fn middle_wildcard_matches() {
        let r = rule("*/v1/*", Phase::Both);
        assert!(r.matches_target("api.example.com/v1/users"));
        assert!(!r.matches_target("api.example.com/v2/users"));
    }

    #[test]
    fn dots_are_escaped() {
        let r = rule("api.example.com/x", Phase::Request);
        assert!(!r.matches_target("apiXexample.com/x"));
    }

    #[test]
    fn applies_respects_enabled_and_phase() {
        let mut r = rule("*", Phase::Request);
        assert!(r.applies(Phase::Request, "any"));
        assert!(!r.applies(Phase::Response, "any"));
        r.enabled = false;
        assert!(!r.applies(Phase::Request, "any"));
    }

    #[test]
    fn rules_roundtrip_to_disk() {
        let tmp = std::env::temp_dir().join(format!("httpcatch-rules-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(load_rules(&tmp).unwrap().is_empty());
        let rules = vec![rule("api.example.com/*", Phase::Both)];
        save_rules(&tmp, &rules).unwrap();
        let back = load_rules(&tmp).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].pattern, "api.example.com/*");
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn library_defaults_then_persists() {
        let tmp = std::env::temp_dir().join(format!("httpcatch-lib-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(load_library(&tmp).unwrap().contains("Библиотека"));
        save_library(&tmp, "function f(){}").unwrap();
        assert_eq!(load_library(&tmp).unwrap(), "function f(){}");
        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
