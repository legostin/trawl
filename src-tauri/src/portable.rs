//! Moving a Trawl setup between machines, or handing it to a colleague.
//!
//! One file carries projects, rules, breakpoints and snippets. Values of
//! project variables never travel — only their names, so the receiver can see
//! what to fill in. Keychain secrets, git-host tokens and captured traffic are
//! not in the document at all.

use serde::{Deserialize, Serialize};

use crate::breakpoints::Breakpoint;
use crate::projects::{EnvVar, Project, ProjectsFile};
use crate::rules::{Phase, Rule};
use crate::snippets::SnippetItem;

/// Bumped when the document's shape changes in a way older apps cannot read.
pub const FORMAT_VERSION: u32 = 1;
const KIND: &str = "trawl-config";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRule {
    pub name: String,
    pub enabled: bool,
    pub pattern: String,
    pub phase: Phase,
    pub script: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportBreakpoint {
    pub name: String,
    pub enabled: bool,
    pub pattern: String,
    #[serde(default)]
    pub method: Option<String>,
    pub on_request: bool,
    pub on_response: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSnippet {
    pub label: String,
    pub code: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportProject {
    pub name: String,
    pub include_hosts: Vec<String>,
    pub exclude_hosts: Vec<String>,
    /// Names only. A value could be a token, and this file is meant to be shared.
    pub env_keys: Vec<String>,
    pub rules: Vec<ExportRule>,
    pub breakpoints: Vec<ExportBreakpoint>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportGlobal {
    #[serde(default)]
    pub rules: Vec<ExportRule>,
    #[serde(default)]
    pub breakpoints: Vec<ExportBreakpoint>,
    #[serde(default)]
    pub snippets: Vec<ExportSnippet>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportDoc {
    pub kind: String,
    pub version: u32,
    pub exported_at: u64,
    pub app: String,
    #[serde(default)]
    pub projects: Vec<ExportProject>,
    #[serde(default)]
    pub global: ExportGlobal,
}

impl ExportDoc {
    pub fn is_empty(&self) -> bool {
        self.projects.is_empty()
            && self.global.rules.is_empty()
            && self.global.breakpoints.is_empty()
            && self.global.snippets.is_empty()
    }
}

/// What an import brought in, for telling the user in words.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub projects: usize,
    pub rules: usize,
    pub breakpoints: usize,
    pub snippets: usize,
    /// Projects that arrived under a different name because theirs was taken.
    pub renamed: Vec<String>,
}

fn rule_out(r: &Rule) -> ExportRule {
    ExportRule {
        name: r.name.clone(),
        enabled: r.enabled,
        pattern: r.pattern.clone(),
        phase: r.phase,
        script: r.script.clone(),
    }
}

fn bp_out(b: &Breakpoint) -> ExportBreakpoint {
    ExportBreakpoint {
        name: b.name.clone(),
        enabled: b.enabled,
        pattern: b.pattern.clone(),
        method: b.method.clone(),
        on_request: b.on_request,
        on_response: b.on_response,
    }
}

/// Build the document.
///
/// `only` limits it to one project — the same shape, just less in it, so the
/// importer never has to know which of the two it was given. Ids are dropped:
/// they are machine-local, and carrying them across would invite collisions.
pub fn build_export(
    projects: &ProjectsFile,
    rules: &[Rule],
    breakpoints: &[Breakpoint],
    snippets: &[SnippetItem],
    only: Option<&str>,
    app_version: &str,
    now: u64,
) -> ExportDoc {
    let wanted: Vec<&Project> = projects
        .projects
        .iter()
        .filter(|p| only.is_none_or(|id| p.id == id))
        .collect();

    let out_projects = wanted
        .iter()
        .map(|p| ExportProject {
            name: p.name.clone(),
            include_hosts: p.include_hosts.clone(),
            exclude_hosts: p.exclude_hosts.clone(),
            env_keys: p.env.iter().map(|e| e.key.clone()).collect(),
            rules: rules
                .iter()
                .filter(|r| r.project_id.as_deref() == Some(p.id.as_str()))
                .map(rule_out)
                .collect(),
            breakpoints: breakpoints
                .iter()
                .filter(|b| b.project_id.as_deref() == Some(p.id.as_str()))
                .map(bp_out)
                .collect(),
        })
        .collect();

    // Exporting one project carries that project alone; the global shelf
    // belongs to the machine, not to it.
    let global = if only.is_some() {
        ExportGlobal::default()
    } else {
        ExportGlobal {
            rules: rules.iter().filter(|r| r.project_id.is_none()).map(rule_out).collect(),
            breakpoints: breakpoints
                .iter()
                .filter(|b| b.project_id.is_none())
                .map(bp_out)
                .collect(),
            snippets: snippets
                .iter()
                .map(|s| ExportSnippet {
                    label: s.label.clone(),
                    code: s.code.clone(),
                    kind: s.kind.clone(),
                })
                .collect(),
        }
    };

    ExportDoc {
        kind: KIND.to_string(),
        version: FORMAT_VERSION,
        exported_at: now,
        app: app_version.to_string(),
        projects: out_projects,
        global,
    }
}

/// Read a document, refusing anything this app cannot honestly apply.
pub fn parse_export(text: &str) -> Result<ExportDoc, String> {
    let doc: ExportDoc =
        serde_json::from_str(text).map_err(|e| format!("this is not a Trawl config file: {e}"))?;
    if doc.kind != KIND {
        return Err("this is not a Trawl config file.".into());
    }
    if doc.version > FORMAT_VERSION {
        return Err(format!(
            "the file comes from a newer Trawl (format {}, this app reads {FORMAT_VERSION}).",
            doc.version
        ));
    }
    if doc.is_empty() {
        return Err("there is nothing in this file to import.".into());
    }
    Ok(doc)
}

/// `name`, or `name (imported)`, or `name (imported 2)` — whatever is free.
fn free_name(name: &str, taken: &[String]) -> String {
    if !taken.iter().any(|t| t == name) {
        return name.to_string();
    }
    let first = format!("{name} (imported)");
    if !taken.iter().any(|t| t == &first) {
        return first;
    }
    (2..)
        .map(|n| format!("{name} (imported {n})"))
        .find(|candidate| !taken.iter().any(|t| t == candidate))
        .unwrap_or(first)
}

/// Apply a document to the current state.
///
/// Nothing is overwritten and nothing is deleted: a project whose name is taken
/// arrives under a free one, and global items are appended. When people trade
/// config files, the one behaviour that must never be destructive by accident
/// is this one.
pub fn apply_import(
    doc: &ExportDoc,
    projects: &mut ProjectsFile,
    rules: &mut Vec<Rule>,
    breakpoints: &mut Vec<Breakpoint>,
    snippets: &mut Vec<SnippetItem>,
    mut new_id: impl FnMut() -> String,
) -> ImportSummary {
    let mut summary = ImportSummary::default();

    for incoming in &doc.projects {
        let taken: Vec<String> = projects.projects.iter().map(|p| p.name.clone()).collect();
        let name = free_name(&incoming.name, &taken);
        if name != incoming.name {
            summary.renamed.push(name.clone());
        }
        let id = new_id();
        projects.projects.push(Project {
            id: id.clone(),
            name,
            include_hosts: incoming.include_hosts.clone(),
            exclude_hosts: incoming.exclude_hosts.clone(),
            // Names arrive empty on purpose: the sender's values were never in
            // the file, and a blank makes it obvious what to fill in.
            env: incoming
                .env_keys
                .iter()
                .map(|key| EnvVar {
                    key: key.clone(),
                    value: String::new(),
                })
                .collect(),
            code_dir: None,
            code_write: false,
        });
        summary.projects += 1;

        for r in &incoming.rules {
            rules.push(into_rule(r, Some(id.clone()), new_id()));
            summary.rules += 1;
        }
        for b in &incoming.breakpoints {
            breakpoints.push(into_bp(b, Some(id.clone()), new_id()));
            summary.breakpoints += 1;
        }
    }

    for r in &doc.global.rules {
        rules.push(into_rule(r, None, new_id()));
        summary.rules += 1;
    }
    for b in &doc.global.breakpoints {
        breakpoints.push(into_bp(b, None, new_id()));
        summary.breakpoints += 1;
    }
    for s in &doc.global.snippets {
        snippets.push(SnippetItem {
            id: new_id(),
            label: s.label.clone(),
            code: s.code.clone(),
            kind: s.kind.clone(),
        });
        summary.snippets += 1;
    }

    summary
}

fn into_rule(r: &ExportRule, project_id: Option<String>, id: String) -> Rule {
    Rule {
        id,
        name: r.name.clone(),
        enabled: r.enabled,
        pattern: r.pattern.clone(),
        phase: r.phase,
        script: r.script.clone(),
        project_id,
    }
}

fn into_bp(b: &ExportBreakpoint, project_id: Option<String>, id: String) -> Breakpoint {
    Breakpoint {
        id,
        name: b.name.clone(),
        enabled: b.enabled,
        pattern: b.pattern.clone(),
        method: b.method.clone(),
        on_request: b.on_request,
        on_response: b.on_response,
        project_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(id: &str, name: &str) -> Project {
        Project {
            id: id.into(),
            name: name.into(),
            include_hosts: vec!["api.example.com".into()],
            exclude_hosts: vec![],
            env: vec![
                EnvVar { key: "TOKEN".into(), value: "sk-live-secret".into() },
                EnvVar { key: "BASE_URL".into(), value: "https://api.example.com".into() },
            ],
            code_dir: Some("/Users/me/repo".into()),
            code_write: true,
        }
    }

    fn rule(id: &str, name: &str, project_id: Option<&str>) -> Rule {
        Rule {
            id: id.into(),
            name: name.into(),
            enabled: true,
            pattern: "api.example.com/*".into(),
            phase: Phase::Request,
            script: "setHeader(request,'x','1');".into(),
            project_id: project_id.map(Into::into),
        }
    }

    fn bp(id: &str, project_id: Option<&str>) -> Breakpoint {
        Breakpoint {
            id: id.into(),
            name: "login".into(),
            enabled: false,
            pattern: "*/login".into(),
            method: None,
            on_request: true,
            on_response: false,
            project_id: project_id.map(Into::into),
        }
    }

    fn snippet(id: &str) -> SnippetItem {
        SnippetItem { id: id.into(), label: "log".into(), code: "log(1)".into(), kind: "snippet".into() }
    }

    fn state() -> (ProjectsFile, Vec<Rule>, Vec<Breakpoint>, Vec<SnippetItem>) {
        let file = ProjectsFile {
            projects: vec![project("p1", "checkout"), project("p2", "admin")],
            active_id: Some("p1".into()),
            global_env: vec![],
        };
        (
            file,
            vec![rule("r1", "add auth", Some("p1")), rule("r2", "shared", None)],
            vec![bp("b1", Some("p1")), bp("b2", None)],
            vec![snippet("s1")],
        )
    }

    fn ids() -> impl FnMut() -> String {
        let mut n = 0;
        move || {
            n += 1;
            format!("new{n}")
        }
    }

    #[test]
    fn variable_values_never_travel_but_their_names_do() {
        // The file is meant to be shared; a value could be a token.
        let (p, r, b, s) = state();
        let doc = build_export(&p, &r, &b, &s, None, "0.17.0", 1);
        let json = serde_json::to_string(&doc).unwrap();
        assert!(!json.contains("sk-live-secret"), "a variable value reached the file");
        assert_eq!(doc.projects[0].env_keys, vec!["TOKEN", "BASE_URL"]);
    }

    #[test]
    fn identifiers_and_machine_local_settings_stay_behind() {
        let (p, r, b, s) = state();
        let json = serde_json::to_string(&build_export(&p, &r, &b, &s, None, "0.17.0", 1)).unwrap();
        for local in ["\"p1\"", "\"r1\"", "\"b1\"", "/Users/me/repo"] {
            assert!(!json.contains(local), "{local} should not be in the document");
        }
    }

    #[test]
    fn exporting_one_project_carries_that_project_alone() {
        // The global shelf belongs to the machine, not to the project.
        let (p, r, b, s) = state();
        let doc = build_export(&p, &r, &b, &s, Some("p1"), "0.17.0", 1);
        assert_eq!(doc.projects.len(), 1);
        assert_eq!(doc.projects[0].name, "checkout");
        assert_eq!(doc.projects[0].rules.len(), 1);
        assert_eq!(doc.global, ExportGlobal::default());
    }

    #[test]
    fn exporting_everything_separates_global_from_project() {
        let (p, r, b, s) = state();
        let doc = build_export(&p, &r, &b, &s, None, "0.17.0", 1);
        assert_eq!(doc.projects.len(), 2);
        assert_eq!(doc.global.rules.len(), 1);
        assert_eq!(doc.global.rules[0].name, "shared");
        assert_eq!(doc.global.breakpoints.len(), 1);
        assert_eq!(doc.global.snippets.len(), 1);
    }

    #[test]
    fn a_round_trip_lands_the_same_rules_in_the_same_places() {
        let (p, r, b, s) = state();
        let doc = build_export(&p, &r, &b, &s, None, "0.17.0", 1);
        let text = serde_json::to_string(&doc).unwrap();

        let (mut p2, mut r2, mut b2, mut s2) =
            (ProjectsFile::default(), vec![], vec![], vec![]);
        let parsed = parse_export(&text).unwrap();
        let summary = apply_import(&parsed, &mut p2, &mut r2, &mut b2, &mut s2, ids());

        assert_eq!(summary.projects, 2);
        // One rule belongs to checkout, one is global; admin has none.
        assert_eq!(summary.rules, 2);
        let imported = p2.projects.iter().find(|x| x.name == "checkout").unwrap();
        assert_eq!(
            r2.iter().filter(|x| x.project_id.as_deref() == Some(&imported.id)).count(),
            1
        );
        assert_eq!(r2.iter().filter(|x| x.project_id.is_none()).count(), 1);
    }

    #[test]
    fn imported_variables_arrive_named_and_empty() {
        let (p, r, b, s) = state();
        let doc = build_export(&p, &r, &b, &s, Some("p1"), "0.17.0", 1);
        let (mut p2, mut r2, mut b2, mut s2) = (ProjectsFile::default(), vec![], vec![], vec![]);
        apply_import(&doc, &mut p2, &mut r2, &mut b2, &mut s2, ids());
        let env = &p2.projects[0].env;
        assert_eq!(env.iter().map(|e| e.key.as_str()).collect::<Vec<_>>(), ["TOKEN", "BASE_URL"]);
        assert!(env.iter().all(|e| e.value.is_empty()), "a value came back from nowhere");
    }

    #[test]
    fn a_taken_name_is_given_a_free_one_rather_than_overwritten() {
        let (p, r, b, s) = state();
        let doc = build_export(&p, &r, &b, &s, Some("p1"), "0.17.0", 1);
        let (mut p2, mut r2, mut b2, mut s2) = state();
        let before = p2.projects.len();

        let summary = apply_import(&doc, &mut p2, &mut r2, &mut b2, &mut s2, ids());
        assert_eq!(p2.projects.len(), before + 1);
        assert_eq!(summary.renamed, vec!["checkout (imported)"]);
        assert!(p2.projects.iter().any(|x| x.name == "checkout"), "the original was replaced");

        // And again, so the second collision does not reuse the first name.
        let summary2 = apply_import(&doc, &mut p2, &mut r2, &mut b2, &mut s2, ids());
        assert_eq!(summary2.renamed, vec!["checkout (imported 2)"]);
    }

    #[test]
    fn a_file_that_is_not_ours_is_refused() {
        assert!(parse_export("{}").is_err());
        assert!(parse_export("not json at all").is_err());
        assert!(parse_export(r#"{"kind":"something-else","version":1,"exportedAt":0,"app":"x"}"#).is_err());
    }

    #[test]
    fn a_newer_format_is_refused_by_name() {
        let text = format!(
            r#"{{"kind":"{KIND}","version":{},"exportedAt":0,"app":"9.9.9","projects":[]}}"#,
            FORMAT_VERSION + 1
        );
        let err = parse_export(&text).unwrap_err();
        assert!(err.contains("newer"), "{err}");
    }

    #[test]
    fn an_empty_document_says_so_instead_of_succeeding_quietly() {
        let text = format!(
            r#"{{"kind":"{KIND}","version":{FORMAT_VERSION},"exportedAt":0,"app":"x","projects":[]}}"#
        );
        assert!(parse_export(&text).unwrap_err().contains("nothing"));
    }
}

// ── Commands ──

fn stamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Write the configuration to `path`. `project_id` limits it to one project.
#[tauri::command]
pub fn export_config(
    app: tauri::AppHandle,
    path: String,
    project_id: Option<String>,
) -> Result<(), String> {
    let data = crate::commands::data_dir(&app)?;
    let rules_dir = crate::commands::rules_dir(&app)?;

    let projects = crate::projects::load_projects(&data).map_err(|e| e.to_string())?;
    let rules = crate::rules::load_rules(&rules_dir).map_err(|e| e.to_string())?;
    let breakpoints = crate::breakpoints::load_breakpoints(&rules_dir).map_err(|e| e.to_string())?;
    let snippets = crate::snippets::load_snippets(&rules_dir).map_err(|e| e.to_string())?;

    let doc = build_export(
        &projects,
        &rules,
        &breakpoints,
        &snippets.items,
        project_id.as_deref(),
        app.package_info().version.to_string().as_str(),
        stamp(),
    );
    let text = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| format!("could not write {path}: {e}"))
}

/// Read `path` and add what is in it. Nothing existing is changed or removed.
#[tauri::command]
pub fn import_config(app: tauri::AppHandle, path: String) -> Result<ImportSummary, String> {
    let text = std::fs::read_to_string(&path).map_err(|e| format!("could not read {path}: {e}"))?;
    // Parsed in full before anything is written: an import lands whole or not
    // at all, rather than leaving half a colleague's setup behind.
    let doc = parse_export(&text)?;

    let data = crate::commands::data_dir(&app)?;
    let rules_dir = crate::commands::rules_dir(&app)?;

    let mut projects = crate::projects::load_projects(&data).map_err(|e| e.to_string())?;
    let mut rules = crate::rules::load_rules(&rules_dir).map_err(|e| e.to_string())?;
    let mut breakpoints =
        crate::breakpoints::load_breakpoints(&rules_dir).map_err(|e| e.to_string())?;
    let mut snippets = crate::snippets::load_snippets(&rules_dir).map_err(|e| e.to_string())?;

    let mut n = 0u64;
    let base = stamp();
    let summary = apply_import(
        &doc,
        &mut projects,
        &mut rules,
        &mut breakpoints,
        &mut snippets.items,
        || {
            n += 1;
            format!("imp-{base}-{n}")
        },
    );

    crate::projects::save_projects(&data, &projects).map_err(|e| e.to_string())?;
    crate::rules::save_rules(&rules_dir, &rules).map_err(|e| e.to_string())?;
    crate::breakpoints::save_breakpoints(&rules_dir, &breakpoints).map_err(|e| e.to_string())?;
    crate::snippets::save_snippets(&rules_dir, &snippets).map_err(|e| e.to_string())?;
    Ok(summary)
}
