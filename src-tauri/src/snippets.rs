//! Persisted user templates & snippets + usage counts (snippets.json in the
//! scripting dir). Built-in defaults live in the frontend; this stores only
//! what the user adds plus how often each item (built-in or custom) is used.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnippetItem {
    pub id: String,
    pub label: String,
    pub code: String,
    /// "template" (replaces the script) or "snippet" (inserts at the cursor).
    pub kind: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnippetsFile {
    #[serde(default)]
    pub items: Vec<SnippetItem>,
    /// itemId → number of times used, for "most used" ordering.
    #[serde(default)]
    pub usage: HashMap<String, u64>,
}

pub fn load_snippets(dir: &Path) -> Result<SnippetsFile> {
    let path = dir.join("snippets.json");
    if !path.exists() {
        return Ok(SnippetsFile::default());
    }
    let text = fs::read_to_string(&path).context("read snippets.json")?;
    serde_json::from_str(&text).context("parse snippets.json")
}

pub fn save_snippets(dir: &Path, file: &SnippetsFile) -> Result<()> {
    fs::create_dir_all(dir).context("create scripting dir")?;
    let text = serde_json::to_string_pretty(file).context("serialize snippets")?;
    fs::write(dir.join("snippets.json"), text).context("write snippets.json")?;
    Ok(())
}
