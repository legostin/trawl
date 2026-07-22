# MCP Server Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Встроенный MCP-сервер в Rust-бэкенде Trawl: доступ агентов к трафику, правилам, проектам, брейкпоинтам + динамические тулы от плагинов.

**Architecture:** Модуль `src-tauri/src/mcp/` (config, core_tools, plugin_bridge, server) на `rmcp` 2.2 (Streamable HTTP через axum, loopback + bearer-токен). Кор-тулы зовут те же общие функции, что и Tauri-команды (общая логика выносится в `rules.rs`/`breakpoints.rs`/`projects.rs`). Плагинные тулы регистрируются из JS через `__TRAWL__.mcp` и вызываются через мост Rust → Tauri event → webview → команда-ответ.

**Tech Stack:** Rust (tauri 2, rmcp 2.2, axum 0.8, tokio), TypeScript/React (vitest).

**Спека:** `docs/superpowers/specs/2026-07-22-mcp-server-design.md` — читать перед началом.

## Global Constraints

- Биндинг сервера ТОЛЬКО на `127.0.0.1`. Дефолтный порт `9910`.
- Каждый HTTP-запрос без валидного `Authorization: Bearer <token>` → 401.
- Конфиг: `mcp.json` в app data dir, формат `{ "enabled": bool, "port": u16, "token": string }`, camelCase.
- Токен: 32 случайных байта, hex (64 символа).
- Имена кор-тулов — snake_case; плагинные тулы — `<pluginId>_<name>`.
- Дефолтный лимит тела в ответах тулов: 50 000 байт, параметр `maxBodyBytes`.
- Таймаут плагинного тула: 60 000 мс по умолчанию, переопределяется `timeoutMs`.
- Никакого дублирования бизнес-логики между Tauri-командами и MCP-тулами — только общие функции.
- Комментарии в коде — в стиле существующих файлов (русский для «зачем», английский допустим).
- После КАЖДОЙ задачи: `cargo test` (из `src-tauri/`) и `pnpm test` зелёные.
- rmcp 2.2 API проверен по docs.rs (сигнатуры ниже точные). Если при сборке сигнатура не совпала — сверяйся с https://docs.rs/rmcp/latest и примером `examples/servers/src/counter_streamhttp.rs` в modelcontextprotocol/rust-sdk, а не изобретай.

## File Structure

```
src-tauri/src/mcp/mod.rs            — McpConfig load/save, gen_token/gen_id, McpState, PeerRegistry, apply_config, config-команды
src-tauri/src/mcp/core_tools.rs     — Deps, ToolDef, core_tools(), dispatch(), flow_to_json
src-tauri/src/mcp/plugin_bridge.rs  — PluginBridge (реестр+pending), команды mcp_register_tool/…/mcp_tool_result
src-tauri/src/mcp/server.rs         — TrawlMcp (ServerHandler), require_bearer, start_server/ServerHandle
src-tauri/src/rules.rs              — + upsert_rule, remove_rule (общие с UI)
src-tauri/src/breakpoints.rs        — + upsert_breakpoint, remove_breakpoint
src-tauri/src/projects.rs           — + upsert_project, remove_project, set_active
src-tauri/src/commands.rs           — рефакторинг на общие фн, + pub data_dir/rules_dir, + resolve_breakpoint_core, + FlowStore::get
src-tauri/src/store.rs              — + get(id)
src-tauri/src/db.rs                 — + #[derive(Default)] на FlowQuery
src-tauri/src/lib.rs                — mod mcp, manage(McpState), старт сервера в setup, регистрация команд
src/plugins/mcpBridge.ts            — фронтовый мост (реестр handler-ов, событие mcp:tool-call)
src/plugins/api.ts                  — типы McpToolSpec/TrawlMcp, поле mcp в TrawlHost
src/plugins/host.ts                 — секция mcp, initMcpBridge()
src/plugins/loader.ts               — setLoadingPlugin вокруг инъекции, clearPluginTools перед перезагрузкой
src/plugins.ts                      — clearPluginTools при disable
src/mcp.ts                          — invoke-обёртки конфиг-команд, mcpAddCommand()
src/components/McpSection.tsx       — блок MCP в SetupPanel
docs/plugins.md                     — раздел про __TRAWL__.mcp
```

---

### Task 1: Зависимости + конфиг-модуль MCP

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/mcp/mod.rs`
- Modify: `src-tauri/src/lib.rs` (только `mod mcp;`)

**Interfaces:**
- Produces: `mcp::McpConfig { enabled: bool, port: u16, token: String }`, `mcp::DEFAULT_PORT: u16 = 9910`, `mcp::load_config(dir: &Path) -> McpConfig`, `mcp::save_config(dir: &Path, cfg: &McpConfig) -> Result<(), String>`, `mcp::gen_token() -> String`, `mcp::gen_id() -> String`.

- [ ] **Step 1: Добавить зависимости**

В `src-tauri/Cargo.toml` в `[dependencies]` добавить:

```toml
rmcp = { version = "2.2", features = ["transport-streamable-http-server"] }
axum = "0.8"
rand = "0.9"
```

И секцию (или дополнить существующую) `[dev-dependencies]`:

```toml
tempfile = "3"
tauri = { version = "2", features = ["test"] }
```

Run: `cd src-tauri && cargo build` — должно собраться (долго при первом разе, качает rmcp/axum).

- [ ] **Step 2: Написать падающий тест конфига**

Создать `src-tauri/src/mcp/mod.rs` пока только с тестами (модуль подключим сразу):

В `src-tauri/src/lib.rs` после `mod httpsend;` добавить строку `mod mcp;`.

Содержимое `src-tauri/src/mcp/mod.rs`:

```rust
//! MCP server: config, state, lifecycle.

use std::fs;
use std::path::Path;

use rand::RngCore;
use serde::{Deserialize, Serialize};

pub const DEFAULT_PORT: u16 = 9910;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpConfig {
    pub enabled: bool,
    pub port: u16,
    pub token: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_and_generates_token_once() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = load_config(dir.path());
        assert!(cfg.enabled);
        assert_eq!(cfg.port, DEFAULT_PORT);
        assert_eq!(cfg.token.len(), 64);
        // повторная загрузка возвращает тот же токен (сохранился на диск)
        let again = load_config(dir.path());
        assert_eq!(again.token, cfg.token);
    }

    #[test]
    fn config_roundtrips_camel_case() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = McpConfig { enabled: false, port: 1234, token: "t".into() };
        save_config(dir.path(), &cfg).unwrap();
        let text = std::fs::read_to_string(dir.path().join("mcp.json")).unwrap();
        assert!(text.contains("\"enabled\": false"), "json was: {text}");
        let back = load_config(dir.path());
        assert!(!back.enabled);
        assert_eq!(back.port, 1234);
        assert_eq!(back.token, "t");
    }

    #[test]
    fn gen_id_is_16_hex_chars() {
        let id = gen_id();
        assert_eq!(id.len(), 16);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
```

- [ ] **Step 3: Убедиться, что тест падает**

Run: `cd src-tauri && cargo test mcp::` — FAIL: `cannot find function load_config` (ошибка компиляции — это и есть красный шаг для Rust).

- [ ] **Step 4: Реализовать**

Дописать в `src-tauri/src/mcp/mod.rs` (между структурой и тестами):

```rust
impl Default for McpConfig {
    fn default() -> Self {
        McpConfig { enabled: true, port: DEFAULT_PORT, token: String::new() }
    }
}

pub fn gen_token() -> String {
    let mut b = [0u8; 32];
    rand::rng().fill_bytes(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Короткий id для сущностей, созданных через MCP (у UI — crypto.randomUUID).
pub fn gen_id() -> String {
    let mut b = [0u8; 8];
    rand::rng().fill_bytes(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Читает mcp.json; при отсутствии файла или пустом токене — генерирует токен
/// и сразу сохраняет, чтобы он был стабилен между запусками.
pub fn load_config(dir: &Path) -> McpConfig {
    let mut cfg: McpConfig = fs::read_to_string(dir.join("mcp.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    if cfg.token.is_empty() {
        cfg.token = gen_token();
        let _ = save_config(dir, &cfg);
    }
    cfg
}

pub fn save_config(dir: &Path, cfg: &McpConfig) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    fs::write(dir.join("mcp.json"), text).map_err(|e| e.to_string())
}
```

- [ ] **Step 5: Тесты зелёные**

Run: `cd src-tauri && cargo test` — все проходят (включая старые 85).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/mcp/mod.rs src-tauri/src/lib.rs
git commit -m "feat(mcp): config module — mcp.json, token generation"
```

---

### Task 2: Общие функции ядра (rules/breakpoints/projects/resolve)

Валидация и сохранение сейчас живут внутри Tauri-команд. Выносим в модули, чтобы MCP-тулы использовали ровно ту же логику.

**Files:**
- Modify: `src-tauri/src/rules.rs`
- Modify: `src-tauri/src/breakpoints.rs`
- Modify: `src-tauri/src/projects.rs`
- Modify: `src-tauri/src/commands.rs` (команды `save_rule`, `delete_rule`, `save_breakpoint`, `delete_breakpoint`, `save_project`, `delete_project`, `set_active_project`, `resolve_breakpoint` становятся тонкими обёртками; `data_dir`/`rules_dir` делаем `pub`)

**Interfaces:**
- Produces:
  - `rules::upsert_rule(dir: &Path, rule: Rule) -> Result<Vec<Rule>, String>` (валидация конфликтов фаз, как в UI)
  - `rules::remove_rule(dir: &Path, id: &str) -> Result<Vec<Rule>, String>`
  - `breakpoints::upsert_breakpoint(dir: &Path, bp: Breakpoint) -> Result<Vec<Breakpoint>, String>`
  - `breakpoints::remove_breakpoint(dir: &Path, id: &str) -> Result<Vec<Breakpoint>, String>`
  - `projects::upsert_project(dir: &Path, project: Project) -> Result<ProjectsFile, String>`
  - `projects::remove_project(dir: &Path, id: &str) -> Result<ProjectsFile, String>`
  - `projects::set_active(dir: &Path, id: Option<String>) -> Result<Option<Project>, String>` (возвращает резолвнутый активный проект)
  - `commands::resolve_breakpoint_core(pending: &BreakpointRegistry, id: u64, phase: &str, action: &str, edited: EditedPayload) -> Result<(), String>`
  - `commands::data_dir(app: &AppHandle) -> Result<PathBuf, String>` (теперь `pub`), `commands::rules_dir(app: &AppHandle) -> Result<PathBuf, String>` (теперь `pub`)

- [ ] **Step 1: Падающие тесты**

В `src-tauri/src/rules.rs` в конец `mod tests` (модуль тестов там уже есть — если нет, создать стандартный) добавить:

```rust
#[test]
fn upsert_rule_rejects_conflicting_enabled_pair() {
    let dir = tempfile::tempdir().unwrap();
    let a = Rule {
        id: "a".into(), name: "A".into(), enabled: true,
        pattern: "api.example.com/*".into(), phase: Phase::Request,
        script: String::new(), project_id: None,
    };
    upsert_rule(dir.path(), a).unwrap();
    let b = Rule {
        id: "b".into(), name: "B".into(), enabled: true,
        pattern: "api.example.com/*".into(), phase: Phase::Both,
        script: String::new(), project_id: None,
    };
    let err = upsert_rule(dir.path(), b).unwrap_err();
    assert!(err.contains("Conflicts"), "err was: {err}");
}

#[test]
fn upsert_rule_updates_existing_and_remove_deletes() {
    let dir = tempfile::tempdir().unwrap();
    let a = Rule {
        id: "a".into(), name: "A".into(), enabled: true,
        pattern: "x/*".into(), phase: Phase::Request,
        script: String::new(), project_id: None,
    };
    upsert_rule(dir.path(), a.clone()).unwrap();
    let renamed = Rule { name: "A2".into(), ..a };
    let rules = upsert_rule(dir.path(), renamed).unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].name, "A2");
    let rules = remove_rule(dir.path(), "a").unwrap();
    assert!(rules.is_empty());
}
```

Аналогичный тест в `src-tauri/src/breakpoints.rs` `mod tests`:

```rust
#[test]
fn upsert_breakpoint_rejects_same_pattern_method_phase() {
    let dir = tempfile::tempdir().unwrap();
    let a = Breakpoint {
        id: "a".into(), name: "A".into(), enabled: true,
        pattern: "*/login".into(), method: None,
        on_request: true, on_response: false, project_id: None,
    };
    upsert_breakpoint(dir.path(), a).unwrap();
    let b = Breakpoint {
        id: "b".into(), name: "B".into(), enabled: true,
        pattern: "*/login".into(), method: None,
        on_request: true, on_response: false, project_id: None,
    };
    let err = upsert_breakpoint(dir.path(), b).unwrap_err();
    assert!(err.contains("Conflicts"), "err was: {err}");
}
```

И в `src-tauri/src/projects.rs` `mod tests`:

```rust
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
```

- [ ] **Step 2: Убедиться, что не компилируется**

Run: `cd src-tauri && cargo test` — FAIL: `cannot find function upsert_rule` и т.д.

- [ ] **Step 3: Реализовать общие функции**

`src-tauri/src/rules.rs` — перенести тело валидации из `commands::save_rule`:

```rust
/// Вставляет/обновляет правило с той же валидацией, что UI-команда.
/// Возвращает полный список правил после сохранения.
pub fn upsert_rule(dir: &Path, rule: Rule) -> Result<Vec<Rule>, String> {
    let mut rules = load_rules(dir).map_err(|e| e.to_string())?;
    // Hard validation: no two enabled rules with the same scope + pattern + overlapping phase.
    if rule.enabled {
        if let Some(other) = rules.iter().find(|r| {
            r.id != rule.id
                && r.enabled
                && r.project_id == rule.project_id
                && r.pattern == rule.pattern
                && phases_conflict(r.phase, rule.phase)
        }) {
            return Err(format!(
                "Conflicts with enabled rule “{}” — same pattern and phase. Disable it first.",
                other.name
            ));
        }
    }
    if let Some(existing) = rules.iter_mut().find(|r| r.id == rule.id) {
        *existing = rule;
    } else {
        rules.push(rule);
    }
    save_rules(dir, &rules).map_err(|e| e.to_string())?;
    Ok(rules)
}

pub fn remove_rule(dir: &Path, id: &str) -> Result<Vec<Rule>, String> {
    let mut rules = load_rules(dir).map_err(|e| e.to_string())?;
    rules.retain(|r| r.id != id);
    save_rules(dir, &rules).map_err(|e| e.to_string())?;
    Ok(rules)
}
```

`src-tauri/src/breakpoints.rs` — перенести валидацию из `commands::save_breakpoint`:

```rust
/// Вставляет/обновляет брейкпоинт с той же валидацией, что UI-команда.
pub fn upsert_breakpoint(dir: &Path, breakpoint: Breakpoint) -> Result<Vec<Breakpoint>, String> {
    let mut bps = load_breakpoints(dir).map_err(|e| e.to_string())?;
    // Hard validation: no two enabled breakpoints with the same scope + pattern +
    // method that also pause the same phase (both request, or both response).
    if breakpoint.enabled {
        if let Some(other) = bps.iter().find(|b| {
            b.id != breakpoint.id
                && b.enabled
                && b.project_id == breakpoint.project_id
                && b.pattern == breakpoint.pattern
                && b.method == breakpoint.method
                && ((b.on_request && breakpoint.on_request)
                    || (b.on_response && breakpoint.on_response))
        }) {
            return Err(format!(
                "Conflicts with enabled breakpoint “{}” — same pattern, method and phase. Disable it first.",
                other.name
            ));
        }
    }
    if let Some(existing) = bps.iter_mut().find(|b| b.id == breakpoint.id) {
        *existing = breakpoint;
    } else {
        bps.push(breakpoint);
    }
    save_breakpoints(dir, &bps).map_err(|e| e.to_string())?;
    Ok(bps)
}

pub fn remove_breakpoint(dir: &Path, id: &str) -> Result<Vec<Breakpoint>, String> {
    let mut bps = load_breakpoints(dir).map_err(|e| e.to_string())?;
    bps.retain(|b| b.id != id);
    save_breakpoints(dir, &bps).map_err(|e| e.to_string())?;
    Ok(bps)
}
```

`src-tauri/src/projects.rs` — перенести из `commands::save_project`/`delete_project`/`set_active_project` (часть с состоянием остаётся в командах):

```rust
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
```

- [ ] **Step 4: Рефакторинг команд**

В `src-tauri/src/commands.rs`:

1. `fn data_dir` и `fn rules_dir` → `pub fn`.
2. Команды переписать на общие функции (поведение 1:1, включая обновление разделяемого состояния):

```rust
#[tauri::command]
pub fn save_rule(app: AppHandle, rule: Rule, state: State<'_, AppState>) -> Result<Vec<Rule>, String> {
    let rules = rules::upsert_rule(&rules_dir(&app)?, rule)?;
    *state.rules.write().unwrap() = rules.clone();
    Ok(rules)
}

#[tauri::command]
pub fn delete_rule(app: AppHandle, id: String, state: State<'_, AppState>) -> Result<Vec<Rule>, String> {
    let rules = rules::remove_rule(&rules_dir(&app)?, &id)?;
    *state.rules.write().unwrap() = rules.clone();
    Ok(rules)
}
```

Аналогично `save_breakpoint`/`delete_breakpoint` через `crate::breakpoints::upsert_breakpoint`/`remove_breakpoint` (с записью в `state.breakpoints`).

`save_project`:

```rust
#[tauri::command]
pub fn save_project(
    app: AppHandle,
    project: Project,
    state: State<'_, AppState>,
) -> Result<ProjectsFile, String> {
    let file = projects::upsert_project(&data_dir(&app)?, project.clone())?;
    // если правим активный проект — обновить общую ячейку
    let mut active = state.active_project.write().unwrap();
    if active.as_ref().map(|p| &p.id) == Some(&project.id) {
        *active = Some(project);
    }
    Ok(file)
}

#[tauri::command]
pub fn delete_project(
    app: AppHandle,
    id: String,
    state: State<'_, AppState>,
) -> Result<ProjectsFile, String> {
    let file = projects::remove_project(&data_dir(&app)?, &id)?;
    if file.active_id.is_none() {
        let mut active = state.active_project.write().unwrap();
        if active.as_ref().map(|p| p.id.as_str()) == Some(id.as_str()) {
            *active = None;
        }
    }
    Ok(file)
}

#[tauri::command]
pub fn set_active_project(
    app: AppHandle,
    id: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let resolved = projects::set_active(&data_dir(&app)?, id)?;
    *state.active_project.write().unwrap() = resolved;
    Ok(())
}
```

`resolve_breakpoint` — вынести тело в pub-функцию, команда становится обёрткой:

```rust
/// Общее ядро resolve: используется Tauri-командой и MCP-тулом.
pub fn resolve_breakpoint_core(
    pending: &crate::proxy::BreakpointRegistry,
    id: u64,
    phase: &str,
    action: &str,
    edited: EditedPayload,
) -> Result<(), String> {
    use base64::Engine;
    use crate::proxy::{BpPhase, Resolution};
    let bp_phase = match phase {
        "request" => BpPhase::Request,
        "response" => BpPhase::Response,
        _ => return Err("bad phase".into()),
    };
    // Decode an uploaded file body (base64) into raw bytes, if present.
    let body_bytes = match edited.body_base64 {
        Some(b64) => Some(
            base64::engine::general_purpose::STANDARD
                .decode(b64.as_bytes())
                .map_err(|e| format!("bad base64 body: {e}"))?,
        ),
        None => None,
    };
    let resolution = match action {
        "execute" => Resolution::Execute {
            method: edited.method,
            path: edited.path,
            status: edited.status,
            headers: edited.headers,
            body: edited.body,
            body_bytes,
        },
        "abort" => Resolution::Abort(edited.reason.unwrap_or_else(|| "aborted".into())),
        "respond" => Resolution::Respond {
            status: edited.status.unwrap_or(200),
            headers: edited.headers,
            body: edited.body,
            body_bytes,
        },
        _ => return Err("bad action".into()),
    };
    let tx = pending.lock().unwrap().remove(&(id, bp_phase));
    match tx {
        Some(tx) => {
            let _ = tx.send(resolution);
            Ok(())
        }
        None => Err("no pending breakpoint".into()),
    }
}

#[tauri::command]
pub fn resolve_breakpoint(
    id: u64,
    phase: String,
    action: String,
    edited: EditedPayload,
    state: State<'_, AppState>,
) -> Result<(), String> {
    resolve_breakpoint_core(&state.pending_breakpoints, id, &phase, &action, edited)
}
```

Также сделать `pub fn db(&self)` в `impl AppState` (сейчас приватная — понадобится MCP-тулам).

- [ ] **Step 5: Тесты зелёные**

Run: `cd src-tauri && cargo test` — все проходят. Старые тесты команд не должны сломаться.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src
git commit -m "refactor: extract shared rule/breakpoint/project core from commands"
```

---

### Task 3: core_tools — каркас, статус и трафик

**Files:**
- Create: `src-tauri/src/mcp/core_tools.rs`
- Modify: `src-tauri/src/mcp/mod.rs` (`pub mod core_tools;`)
- Modify: `src-tauri/src/store.rs` (+ `get`)
- Modify: `src-tauri/src/db.rs` (+ `#[derive(Default)]` на `FlowQuery` — у него все поля со `#[serde(default)]`, Default корректен)

**Interfaces:**
- Consumes: `commands::AppState` (pub `db()`), `db::FlowQuery`, `model::Flow`.
- Produces:
  - `core_tools::Deps<'a> { state: &'a AppState, data_dir: PathBuf, rules_dir: PathBuf }`
  - `core_tools::ToolDef { name: &'static str, description: &'static str, schema: serde_json::Value }`
  - `core_tools::core_tools() -> Vec<ToolDef>` — на этом шаге: `get_status`, `query_flows`, `get_flow`, `flow_count`, `aggregate_flows`
  - `core_tools::dispatch(deps: &Deps, name: &str, args: &Value) -> Result<Value, String>`
  - `core_tools::flow_to_json(flow: &Flow, max_body: usize) -> Value`

- [ ] **Step 1: `FlowStore::get` + тест**

В `src-tauri/src/store.rs` добавить в `impl FlowStore`:

```rust
pub fn get(&self, id: u64) -> Option<Flow> {
    self.inner.flows.lock().unwrap().iter().find(|f| f.id == id).cloned()
}
```

В `mod tests` там же:

```rust
#[test]
fn get_returns_flow_by_id() {
    let store = FlowStore::new(10);
    let id = store.next_id();
    store.insert(Flow::new_request(
        id,
        "GET".into(),
        UrlParts { scheme: "http".into(), host: "h".into(), port: 80, path: "/".into() },
        HttpMessage { headers: vec![], body: vec![], body_is_text: true },
    ));
    assert!(store.get(id).is_some());
    assert!(store.get(id + 1).is_none());
}
```

- [ ] **Step 2: Падающие тесты core_tools**

Создать `src-tauri/src/mcp/core_tools.rs` c тестами внизу; в `mod.rs` добавить `pub mod core_tools;`.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::AppState;
    use crate::model::{Flow, HttpMessage, UrlParts};
    use serde_json::json;

    fn test_deps(state: &AppState, tmp: &std::path::Path) -> Deps<'_> {
        Deps {
            state,
            data_dir: tmp.to_path_buf(),
            rules_dir: tmp.join("scripting"),
        }
    }

    fn sample_flow(id: u64, body: &[u8], is_text: bool) -> Flow {
        let mut f = Flow::new_request(
            id,
            "GET".into(),
            UrlParts { scheme: "https".into(), host: "api.test".into(), port: 443, path: "/v1".into() },
            HttpMessage { headers: vec![("A".into(), "b".into())], body: body.to_vec(), body_is_text: is_text },
        );
        f.applied_rules = vec!["r1".into()];
        f
    }

    #[test]
    fn flow_to_json_truncates_text_body() {
        let f = sample_flow(1, b"hello world", true);
        let v = flow_to_json(&f, 5);
        assert_eq!(v["request"]["body"], json!("hello"));
        assert_eq!(v["request"]["truncated"], json!(true));
        assert_eq!(v["request"]["bodySize"], json!(11));
        assert_eq!(v["appliedRules"], json!(["r1"]));
    }

    #[test]
    fn flow_to_json_skips_binary_body() {
        let f = sample_flow(1, &[0u8, 159, 146, 150], false);
        let v = flow_to_json(&f, 50_000);
        assert_eq!(v["request"]["body"], json!(null));
        assert_eq!(v["request"]["binary"], json!(true));
    }

    #[test]
    fn dispatch_get_status_and_get_flow() {
        let state = AppState::new();
        let tmp = tempfile::tempdir().unwrap();
        let id = state.store.next_id();
        state.store.insert(sample_flow(id, b"{}", true));
        let deps = test_deps(&state, tmp.path());

        let status = dispatch(&deps, "get_status", &json!({})).unwrap();
        assert_eq!(status["proxyRunning"], json!(false));
        assert_eq!(status["flowsInMemory"], json!(1));

        let flow = dispatch(&deps, "get_flow", &json!({ "id": id })).unwrap();
        assert_eq!(flow["method"], json!("GET"));

        let err = dispatch(&deps, "get_flow", &json!({ "id": 999 })).unwrap_err();
        assert!(err.contains("not found"), "err was: {err}");

        let err = dispatch(&deps, "nope", &json!({})).unwrap_err();
        assert!(err.contains("unknown tool"), "err was: {err}");
    }

    #[test]
    fn dispatch_query_flows_against_temp_db() {
        let state = AppState::new();
        let tmp = tempfile::tempdir().unwrap();
        let handle = crate::db::DbHandle::open(tmp.path().join("t.db")).unwrap();
        let _ = state.db.set(handle);
        // прогнать один flow через writer
        let mut f = sample_flow(1, b"x", true);
        f.state = crate::model::FlowState::Completed;
        state.db.get().unwrap().insert(&f);
        state.db.get().unwrap().flush();
        let deps = test_deps(&state, tmp.path());

        let out = dispatch(&deps, "query_flows", &json!({ "filter": { "host": "api.test" } })).unwrap();
        assert_eq!(out["flows"].as_array().unwrap().len(), 1);
        let cnt = dispatch(&deps, "flow_count", &json!({})).unwrap();
        assert_eq!(cnt["count"], json!(1));
        let agg = dispatch(&deps, "aggregate_flows", &json!({ "groupBy": "host" })).unwrap();
        assert_eq!(agg["buckets"][0]["key"], json!("api.test"));
    }

    #[test]
    fn every_tool_def_has_object_schema() {
        for def in core_tools() {
            assert_eq!(def.schema["type"], json!("object"), "tool {}", def.name);
        }
    }
}
```

Примечание: сигнатуры записи в БД (`insert`/`flush` у `DbHandle`) сверить с `src-tauri/src/db.rs` — использовать те же методы, которыми пользуется прокси (grep `db.get()` по `proxy.rs`); если метод называется иначе (например `insert_flow`/`sync`), поправить тест, НЕ добавляя новых методов в db.

- [ ] **Step 3: Убедиться, что не компилируется**

Run: `cd src-tauri && cargo test mcp::core_tools` — FAIL (нет `Deps`, `dispatch`, …).

- [ ] **Step 4: Реализация**

Верх `src-tauri/src/mcp/core_tools.rs`:

```rust
//! Кор-тулы MCP: определения (имя/описание/схема) и синхронный диспатч.
//! Deps конструируется из AppHandle в сервере и вручную в тестах —
//! поэтому всё здесь тестируется без Tauri.

use std::path::PathBuf;

use serde_json::{json, Value};

use crate::commands::AppState;
use crate::db::FlowQuery;
use crate::model::{Flow, FlowState};

pub struct Deps<'a> {
    pub state: &'a AppState,
    pub data_dir: PathBuf,
    pub rules_dir: PathBuf,
}

pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub schema: Value,
}

fn obj(props: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": props, "required": required })
}

fn filter_prop() -> Value {
    json!({
        "type": "object",
        "description": "Filter over captured traffic history",
        "properties": {
            "query": { "type": "string", "description": "substring of host+path" },
            "method": { "type": "string" },
            "statusClass": { "type": "string", "description": "2xx | 3xx | 4xx | 5xx | empty" },
            "host": { "type": "string", "description": "exact host" },
            "projectId": { "type": "string" },
            "startTs": { "type": "integer", "description": "unix ms" },
            "endTs": { "type": "integer", "description": "unix ms" }
        }
    })
}

pub fn core_tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "get_status",
            description: "Trawl status: proxy running/address, active project, intercept flag, flow counts.",
            schema: obj(json!({}), &[]),
        },
        ToolDef {
            name: "query_flows",
            description: "Query captured traffic history (SQLite). Returns metadata rows without bodies; use get_flow for full request/response.",
            schema: obj(
                json!({
                    "filter": filter_prop(),
                    "limit": { "type": "integer", "description": "max rows, default 50, cap 500" },
                    "offset": { "type": "integer" }
                }),
                &[],
            ),
        },
        ToolDef {
            name: "get_flow",
            description: "Full flow by id from the in-memory capture (recent traffic): headers, bodies, applied rules, timings. Text bodies are truncated to maxBodyBytes.",
            schema: obj(
                json!({
                    "id": { "type": "integer" },
                    "maxBodyBytes": { "type": "integer", "description": "default 50000" }
                }),
                &["id"],
            ),
        },
        ToolDef {
            name: "flow_count",
            description: "Count flows in history matching a filter.",
            schema: obj(json!({ "filter": filter_prop() }), &[]),
        },
        ToolDef {
            name: "aggregate_flows",
            description: "Aggregate history: groupBy host | status | time | duration. bucket = ms for time/duration grouping.",
            schema: obj(
                json!({
                    "filter": filter_prop(),
                    "groupBy": { "type": "string", "enum": ["host", "status", "time", "duration"] },
                    "bucket": { "type": "integer", "description": "bucket size, default 60000" },
                    "limit": { "type": "integer", "description": "default 50" }
                }),
                &[],
            ),
        },
    ]
}

pub fn dispatch(deps: &Deps, name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "get_status" => tool_get_status(deps),
        "query_flows" => tool_query_flows(deps, args),
        "get_flow" => tool_get_flow(deps, args),
        "flow_count" => tool_flow_count(deps, args),
        "aggregate_flows" => tool_aggregate_flows(deps, args),
        _ => Err(format!("unknown tool: {name}")),
    }
}

// ── helpers ──

fn u64_arg(args: &Value, key: &str) -> Option<u64> {
    args.get(key).and_then(|v| v.as_u64())
}

fn str_arg(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn parse_filter(args: &Value) -> Result<FlowQuery, String> {
    serde_json::from_value(args.get("filter").cloned().unwrap_or_else(|| json!({})))
        .map_err(|e| format!("bad filter: {e}"))
}

fn reader(deps: &Deps) -> Result<crate::db::Db, String> {
    deps.state.db()?.reader().map_err(|e| e.to_string())
}

fn body_json(headers: &[(String, String)], body: &[u8], is_text: bool, max: usize, status: Option<u16>) -> Value {
    let mut v = if is_text {
        let cut = body.len().min(max);
        json!({
            "headers": headers,
            "body": String::from_utf8_lossy(&body[..cut]),
            "bodySize": body.len(),
            "truncated": body.len() > max,
        })
    } else {
        json!({
            "headers": headers,
            "body": Value::Null,
            "binary": true,
            "bodySize": body.len(),
        })
    };
    if let Some(s) = status {
        v["status"] = json!(s);
    }
    v
}

pub fn flow_to_json(flow: &Flow, max_body: usize) -> Value {
    json!({
        "id": flow.id,
        "timestamp": flow.timestamp,
        "method": flow.method,
        "url": serde_json::to_value(&flow.url).unwrap_or(Value::Null),
        "state": serde_json::to_value(&flow.state).unwrap_or(Value::Null),
        "error": flow.error,
        "appliedRules": flow.applied_rules,
        "pausedPhase": flow.paused_phase,
        "timings": serde_json::to_value(&flow.timings).unwrap_or(Value::Null),
        "request": body_json(&flow.request.headers, &flow.request.body, flow.request.body_is_text, max_body, None),
        "response": flow.response.as_ref().map(|r| body_json(&r.headers, &r.body, r.body_is_text, max_body, Some(r.status))),
    })
}

// ── tools ──

fn tool_get_status(deps: &Deps) -> Result<Value, String> {
    let addr = deps.state.proxy.lock().unwrap().as_ref().map(|h| h.local_addr().to_string());
    let active = deps.state.active_project.read().unwrap().clone();
    let db_count = deps
        .state
        .db()
        .ok()
        .and_then(|h| h.reader().ok())
        .and_then(|db| db.count(&FlowQuery::default()).ok());
    Ok(json!({
        "proxyRunning": addr.is_some(),
        "proxyAddr": addr,
        "lanIp": crate::net::lan_ip().map(|ip| ip.to_string()),
        "intercept": *deps.state.intercept.read().unwrap(),
        "activeProject": active.map(|p| json!({ "id": p.id, "name": p.name })),
        "flowsInMemory": deps.state.store.all().len(),
        "flowsInDb": db_count,
    }))
}

fn tool_query_flows(deps: &Deps, args: &Value) -> Result<Value, String> {
    let filter = parse_filter(args)?;
    let limit = u64_arg(args, "limit").unwrap_or(50).min(500) as u32;
    let offset = u64_arg(args, "offset").unwrap_or(0) as u32;
    let rows = reader(deps)?.query(&filter, limit, offset).map_err(|e| e.to_string())?;
    Ok(json!({ "flows": rows, "limit": limit, "offset": offset }))
}

fn tool_get_flow(deps: &Deps, args: &Value) -> Result<Value, String> {
    let id = u64_arg(args, "id").ok_or("missing id")?;
    let max = u64_arg(args, "maxBodyBytes").unwrap_or(50_000) as usize;
    let flow = deps.state.store.get(id).ok_or_else(|| format!("flow {id} not found in memory"))?;
    Ok(flow_to_json(&flow, max))
}

fn tool_flow_count(deps: &Deps, args: &Value) -> Result<Value, String> {
    let filter = parse_filter(args)?;
    let count = reader(deps)?.count(&filter).map_err(|e| e.to_string())?;
    Ok(json!({ "count": count }))
}

fn tool_aggregate_flows(deps: &Deps, args: &Value) -> Result<Value, String> {
    let filter = parse_filter(args)?;
    let group_by = str_arg(args, "groupBy").unwrap_or_else(|| "host".into());
    let bucket = u64_arg(args, "bucket").unwrap_or(60_000);
    let limit = u64_arg(args, "limit").unwrap_or(50) as u32;
    let buckets = reader(deps)?
        .aggregate(&filter, &group_by, bucket, limit)
        .map_err(|e| e.to_string())?;
    Ok(json!({ "buckets": buckets, "groupBy": group_by }))
}
```

Плюс: в `src-tauri/src/db.rs` на `FlowQuery` добавить `Default` в derive; в `commands.rs` — `pub fn db(&self)` (сделано в Task 2). Тип возврата `reader()` сверить по db.rs (`Db`).

- [ ] **Step 5: Тесты зелёные**

Run: `cd src-tauri && cargo test` — PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src
git commit -m "feat(mcp): core tools — status, flows, analytics"
```

---

### Task 4: core_tools — правила, проекты, справка по скриптам

**Files:**
- Modify: `src-tauri/src/mcp/core_tools.rs`

**Interfaces:**
- Consumes: `rules::upsert_rule/remove_rule/load_rules/load_library`, `projects::*` (Task 2), `mcp::gen_id()`.
- Produces: тулы `list_rules`, `save_rule`, `delete_rule`, `get_scripting_reference`, `list_projects`, `save_project`, `delete_project`, `set_active_project` в `core_tools()`/`dispatch`.

- [ ] **Step 1: Падающие тесты**

Добавить в `mod tests` core_tools.rs:

```rust
#[test]
fn save_rule_generates_id_and_updates_shared_state() {
    let state = AppState::new();
    let tmp = tempfile::tempdir().unwrap();
    let deps = test_deps(&state, tmp.path());
    let out = dispatch(&deps, "save_rule", &json!({
        "rule": { "name": "R", "pattern": "api.test/*", "phase": "request", "script": "" }
    })).unwrap();
    let rules = out["rules"].as_array().unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0]["enabled"], json!(true));
    assert_eq!(rules[0]["id"].as_str().unwrap().len(), 16);
    // разделяемое состояние обновилось — прокси увидит правило сразу
    assert_eq!(state.rules.read().unwrap().len(), 1);

    let id = rules[0]["id"].as_str().unwrap().to_string();
    dispatch(&deps, "delete_rule", &json!({ "id": id })).unwrap();
    assert!(state.rules.read().unwrap().is_empty());
}

#[test]
fn save_rule_conflict_is_returned_as_error() {
    let state = AppState::new();
    let tmp = tempfile::tempdir().unwrap();
    let deps = test_deps(&state, tmp.path());
    dispatch(&deps, "save_rule", &json!({
        "rule": { "name": "A", "pattern": "x/*", "phase": "both", "script": "" }
    })).unwrap();
    let err = dispatch(&deps, "save_rule", &json!({
        "rule": { "name": "B", "pattern": "x/*", "phase": "request", "script": "" }
    })).unwrap_err();
    assert!(err.contains("Conflicts"), "err was: {err}");
}

#[test]
fn scripting_reference_contains_api_and_library() {
    let state = AppState::new();
    let tmp = tempfile::tempdir().unwrap();
    let deps = test_deps(&state, tmp.path());
    let v = dispatch(&deps, "get_scripting_reference", &json!({})).unwrap();
    assert!(v["apiTypes"].as_str().unwrap().contains("API_DTS"));
    assert!(v["stdlib"].as_str().unwrap().contains("STD_DTS"));
    assert!(v["librarySource"].is_string());
}

#[test]
fn project_tools_roundtrip() {
    let state = AppState::new();
    let tmp = tempfile::tempdir().unwrap();
    let deps = test_deps(&state, tmp.path());
    let out = dispatch(&deps, "save_project", &json!({
        "project": { "name": "P", "includeHosts": ["api.test"] }
    })).unwrap();
    let id = out["projects"][0]["id"].as_str().unwrap().to_string();

    let act = dispatch(&deps, "set_active_project", &json!({ "id": id })).unwrap();
    assert_eq!(act["active"]["name"], json!("P"));
    assert_eq!(state.active_project.read().unwrap().as_ref().unwrap().name, "P");

    dispatch(&deps, "delete_project", &json!({ "id": id })).unwrap();
    assert!(state.active_project.read().unwrap().is_none());

    let listed = dispatch(&deps, "list_projects", &json!({})).unwrap();
    assert!(listed["projects"].as_array().unwrap().is_empty());
}
```

- [ ] **Step 2: Убедиться в падении**

Run: `cd src-tauri && cargo test mcp::core_tools` — FAIL (unknown tool: save_rule …).

- [ ] **Step 3: Реализация**

В `core_tools()` добавить определения:

```rust
ToolDef {
    name: "list_rules",
    description: "List rewrite rules (glob pattern over host+path, phase, JS script). Optional projectId filter.",
    schema: obj(json!({ "projectId": { "type": "string" } }), &[]),
},
ToolDef {
    name: "save_rule",
    description: "Create or update a rule. Omit rule.id to create (id is generated). phase: request | response | both | handler. Script API: call get_scripting_reference first. Fails if an enabled rule with the same pattern+phase exists.",
    schema: obj(
        json!({
            "rule": {
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "name": { "type": "string" },
                    "enabled": { "type": "boolean", "description": "default true" },
                    "pattern": { "type": "string", "description": "glob over host+path, e.g. api.example.com/*" },
                    "phase": { "type": "string", "enum": ["request", "response", "both", "handler"] },
                    "script": { "type": "string" },
                    "projectId": { "type": ["string", "null"] }
                },
                "required": ["name", "pattern", "phase", "script"]
            }
        },),
        &["rule"],
    ),
},
ToolDef {
    name: "delete_rule",
    description: "Delete a rule by id.",
    schema: obj(json!({ "id": { "type": "string" } }), &["id"]),
},
ToolDef {
    name: "get_scripting_reference",
    description: "Rule scripting reference: ctx API typings, stdlib typings and the shared library source. Read before writing rule scripts.",
    schema: obj(json!({}), &[]),
},
ToolDef {
    name: "list_projects",
    description: "List projects (host include/exclude globs, env vars) and the active project id.",
    schema: obj(json!({}), &[]),
},
ToolDef {
    name: "save_project",
    description: "Create or update a project. Omit project.id to create.",
    schema: obj(
        json!({
            "project": {
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "name": { "type": "string" },
                    "includeHosts": { "type": "array", "items": { "type": "string" } },
                    "excludeHosts": { "type": "array", "items": { "type": "string" } },
                    "env": { "type": "array", "items": { "type": "object", "properties": { "key": { "type": "string" }, "value": { "type": "string" } }, "required": ["key", "value"] } }
                },
                "required": ["name"]
            }
        }),
        &["project"],
    ),
},
ToolDef {
    name: "delete_project",
    description: "Delete a project by id.",
    schema: obj(json!({ "id": { "type": "string" } }), &["id"]),
},
ToolDef {
    name: "set_active_project",
    description: "Set the active project (null id clears it). Capture and rules are scoped by the active project.",
    schema: obj(json!({ "id": { "type": ["string", "null"] } }), &[]),
},
```

(Опечатку `},),` из блока выше не воспроизводить — обычный `}),`.)

Арм-ы `dispatch`:

```rust
"list_rules" => tool_list_rules(deps, args),
"save_rule" => tool_save_rule(deps, args),
"delete_rule" => tool_delete_rule(deps, args),
"get_scripting_reference" => tool_scripting_reference(deps),
"list_projects" => tool_list_projects(deps),
"save_project" => tool_save_project(deps, args),
"delete_project" => tool_delete_project(deps, args),
"set_active_project" => tool_set_active_project(deps, args),
```

Реализации:

```rust
const SCRIPT_API_DTS: &str = include_str!("../../../src/scripting/apiTypes.ts");
const SCRIPT_STDLIB: &str = include_str!("../../../src/scripting/stdlib.ts");

fn tool_list_rules(deps: &Deps, args: &Value) -> Result<Value, String> {
    let rules = crate::rules::load_rules(&deps.rules_dir).map_err(|e| e.to_string())?;
    let filter = str_arg(args, "projectId");
    let rules: Vec<_> = rules
        .into_iter()
        .filter(|r| filter.as_deref().map(|p| r.project_id.as_deref() == Some(p)).unwrap_or(true))
        .collect();
    Ok(json!({ "rules": rules }))
}

fn tool_save_rule(deps: &Deps, args: &Value) -> Result<Value, String> {
    let mut raw = args.get("rule").cloned().ok_or("missing rule")?;
    if raw.get("id").and_then(|v| v.as_str()).map(|s| s.is_empty()).unwrap_or(true) {
        raw["id"] = json!(super::gen_id());
    }
    if raw.get("enabled").is_none() {
        raw["enabled"] = json!(true);
    }
    let rule: crate::rules::Rule = serde_json::from_value(raw).map_err(|e| format!("bad rule: {e}"))?;
    let rules = crate::rules::upsert_rule(&deps.rules_dir, rule)?;
    *deps.state.rules.write().unwrap() = rules.clone();
    Ok(json!({ "rules": rules }))
}

fn tool_delete_rule(deps: &Deps, args: &Value) -> Result<Value, String> {
    let id = str_arg(args, "id").ok_or("missing id")?;
    let rules = crate::rules::remove_rule(&deps.rules_dir, &id)?;
    *deps.state.rules.write().unwrap() = rules.clone();
    Ok(json!({ "rules": rules }))
}

fn tool_scripting_reference(deps: &Deps) -> Result<Value, String> {
    let library = crate::rules::load_library(&deps.rules_dir).unwrap_or_default();
    Ok(json!({
        "apiTypes": SCRIPT_API_DTS,
        "stdlib": SCRIPT_STDLIB,
        "librarySource": library,
    }))
}

fn tool_list_projects(deps: &Deps) -> Result<Value, String> {
    let file = crate::projects::load_projects(&deps.data_dir).map_err(|e| e.to_string())?;
    serde_json::to_value(&file).map_err(|e| e.to_string())
}

fn tool_save_project(deps: &Deps, args: &Value) -> Result<Value, String> {
    let mut raw = args.get("project").cloned().ok_or("missing project")?;
    if raw.get("id").and_then(|v| v.as_str()).map(|s| s.is_empty()).unwrap_or(true) {
        raw["id"] = json!(super::gen_id());
    }
    for key in ["includeHosts", "excludeHosts", "env"] {
        if raw.get(key).is_none() {
            raw[key] = json!([]);
        }
    }
    let project: crate::projects::Project =
        serde_json::from_value(raw).map_err(|e| format!("bad project: {e}"))?;
    let file = crate::projects::upsert_project(&deps.data_dir, project.clone())?;
    // как в UI-команде: правка активного проекта обновляет общую ячейку
    let mut active = deps.state.active_project.write().unwrap();
    if active.as_ref().map(|p| &p.id) == Some(&project.id) {
        *active = Some(project);
    }
    serde_json::to_value(&file).map_err(|e| e.to_string())
}

fn tool_delete_project(deps: &Deps, args: &Value) -> Result<Value, String> {
    let id = str_arg(args, "id").ok_or("missing id")?;
    let file = crate::projects::remove_project(&deps.data_dir, &id)?;
    let mut active = deps.state.active_project.write().unwrap();
    if active.as_ref().map(|p| p.id.as_str()) == Some(id.as_str()) {
        *active = None;
    }
    serde_json::to_value(&file).map_err(|e| e.to_string())
}

fn tool_set_active_project(deps: &Deps, args: &Value) -> Result<Value, String> {
    let id = str_arg(args, "id");
    let resolved = crate::projects::set_active(&deps.data_dir, id)?;
    *deps.state.active_project.write().unwrap() = resolved.clone();
    Ok(json!({ "active": resolved.map(|p| json!({ "id": p.id, "name": p.name })) }))
}
```

Сверить: `load_projects` возвращает `ProjectsFile` — сериализуемый (`Serialize`); `load_library(&dir) -> anyhow::Result<String>` (grep по rules.rs; если сигнатура иная — адаптировать вызов, не меняя rules.rs).

- [ ] **Step 4: Тесты зелёные + commit**

Run: `cd src-tauri && cargo test` — PASS.

```bash
git add src-tauri/src
git commit -m "feat(mcp): rules, projects and scripting-reference tools"
```

---

### Task 5: core_tools — брейкпоинты и отправка запросов

**Files:**
- Modify: `src-tauri/src/mcp/core_tools.rs`

**Interfaces:**
- Consumes: `breakpoints::upsert_breakpoint/remove_breakpoint/load_breakpoints`, `commands::resolve_breakpoint_core`, `commands::EditedPayload`, `httpsend::{SendRequest, send_http}`.
- Produces: тулы `list_breakpoints`, `save_breakpoint`, `delete_breakpoint`, `list_paused`, `resolve_breakpoint`, `send_request`.

- [ ] **Step 1: Падающие тесты**

```rust
#[test]
fn breakpoint_tools_roundtrip() {
    let state = AppState::new();
    let tmp = tempfile::tempdir().unwrap();
    let deps = test_deps(&state, tmp.path());
    let out = dispatch(&deps, "save_breakpoint", &json!({
        "breakpoint": { "name": "B", "pattern": "*/login", "onRequest": true, "onResponse": false }
    })).unwrap();
    let id = out["breakpoints"][0]["id"].as_str().unwrap().to_string();
    assert_eq!(state.breakpoints.read().unwrap().len(), 1);
    dispatch(&deps, "delete_breakpoint", &json!({ "id": id })).unwrap();
    assert!(state.breakpoints.read().unwrap().is_empty());
}

#[test]
fn list_paused_returns_only_paused_flows() {
    let state = AppState::new();
    let tmp = tempfile::tempdir().unwrap();
    let mut f = sample_flow(1, b"x", true);
    f.state = crate::model::FlowState::Paused;
    f.paused_phase = Some("request".into());
    state.store.insert(f);
    state.store.insert(sample_flow(2, b"y", true));
    let deps = test_deps(&state, tmp.path());
    let v = dispatch(&deps, "list_paused", &json!({})).unwrap();
    let paused = v["paused"].as_array().unwrap();
    assert_eq!(paused.len(), 1);
    assert_eq!(paused[0]["id"], json!(1));
    assert_eq!(paused[0]["pausedPhase"], json!("request"));
}

#[tokio::test]
async fn resolve_breakpoint_tool_sends_resolution() {
    use crate::proxy::{BpPhase, Resolution};
    let state = AppState::new();
    let tmp = tempfile::tempdir().unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel();
    state.pending_breakpoints.lock().unwrap().insert((7, BpPhase::Request), tx);
    let deps = test_deps(&state, tmp.path());
    dispatch(&deps, "resolve_breakpoint", &json!({
        "flowId": 7, "phase": "request", "action": "abort",
        "edits": { "reason": "nope" }
    })).unwrap();
    match rx.await.unwrap() {
        Resolution::Abort(r) => assert_eq!(r, "nope"),
        _ => panic!("wrong resolution"),
    }
}

#[test]
fn resolve_breakpoint_missing_flow_errors() {
    let state = AppState::new();
    let tmp = tempfile::tempdir().unwrap();
    let deps = test_deps(&state, tmp.path());
    let err = dispatch(&deps, "resolve_breakpoint", &json!({
        "flowId": 1, "phase": "request", "action": "abort", "edits": {}
    })).unwrap_err();
    assert!(err.contains("no pending breakpoint"), "err was: {err}");
}
```

`send_request` не тестируем юнитом (реальная сеть); проверяется вручную в Task 10.

- [ ] **Step 2: FAIL → Step 3: Реализация**

Run: `cd src-tauri && cargo test mcp::core_tools` — FAIL. Затем определения:

```rust
ToolDef {
    name: "list_breakpoints",
    description: "List breakpoint definitions (glob pattern, method, request/response phase).",
    schema: obj(json!({}), &[]),
},
ToolDef {
    name: "save_breakpoint",
    description: "Create or update a breakpoint definition. Omit breakpoint.id to create. Fails on a conflicting enabled breakpoint (same pattern+method+phase).",
    schema: obj(
        json!({
            "breakpoint": {
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "name": { "type": "string" },
                    "enabled": { "type": "boolean", "description": "default true" },
                    "pattern": { "type": "string", "description": "glob over host+path" },
                    "method": { "type": ["string", "null"], "description": "HTTP method filter, null = any" },
                    "onRequest": { "type": "boolean" },
                    "onResponse": { "type": "boolean" },
                    "projectId": { "type": ["string", "null"] }
                },
                "required": ["name", "pattern", "onRequest", "onResponse"]
            }
        }),
        &["breakpoint"],
    ),
},
ToolDef {
    name: "delete_breakpoint",
    description: "Delete a breakpoint definition by id.",
    schema: obj(json!({ "id": { "type": "string" } }), &["id"]),
},
ToolDef {
    name: "list_paused",
    description: "Flows currently paused on a breakpoint, with full request/response so you can decide what to edit. Resolve with resolve_breakpoint.",
    schema: obj(json!({ "maxBodyBytes": { "type": "integer", "description": "default 50000" } }), &[]),
},
ToolDef {
    name: "resolve_breakpoint",
    description: "Resolve a paused flow. action: execute (forward with edits), respond (answer without forwarding), abort. For execute/respond, `headers` REPLACES the full header list — take it from list_paused and modify. body is a string; bodyBase64 overrides it for binary.",
    schema: obj(
        json!({
            "flowId": { "type": "integer" },
            "phase": { "type": "string", "enum": ["request", "response"] },
            "action": { "type": "string", "enum": ["execute", "respond", "abort"] },
            "edits": {
                "type": "object",
                "properties": {
                    "method": { "type": "string" },
                    "path": { "type": "string", "description": "request path+query (request phase)" },
                    "status": { "type": "integer" },
                    "headers": { "type": "array", "items": { "type": "array", "prefixItems": [{ "type": "string" }, { "type": "string" }] } },
                    "body": { "type": "string" },
                    "bodyBase64": { "type": "string" },
                    "reason": { "type": "string", "description": "abort reason" }
                }
            }
        }),
        &["flowId", "phase", "action"],
    ),
},
ToolDef {
    name: "send_request",
    description: "Send a one-shot HTTP request (like the UI composer). viaProxy=true routes it through the local proxy so it shows up in the capture.",
    schema: obj(
        json!({
            "method": { "type": "string" },
            "url": { "type": "string" },
            "headers": { "type": "array", "items": { "type": "array", "prefixItems": [{ "type": "string" }, { "type": "string" }] } },
            "body": { "type": "string" },
            "bodyB64": { "type": "string", "description": "base64 raw body, overrides body" },
            "viaProxy": { "type": "boolean", "description": "default false" },
            "maxBodyBytes": { "type": "integer", "description": "default 50000" }
        }),
        &["method", "url"],
    ),
},
```

Арм-ы диспатча + реализации:

```rust
"list_breakpoints" => tool_list_breakpoints(deps),
"save_breakpoint" => tool_save_breakpoint(deps, args),
"delete_breakpoint" => tool_delete_breakpoint(deps, args),
"list_paused" => tool_list_paused(deps, args),
"resolve_breakpoint" => tool_resolve_breakpoint(deps, args),
"send_request" => tool_send_request(deps, args),
```

```rust
fn tool_list_breakpoints(deps: &Deps) -> Result<Value, String> {
    let bps = crate::breakpoints::load_breakpoints(&deps.rules_dir).map_err(|e| e.to_string())?;
    Ok(json!({ "breakpoints": bps }))
}

fn tool_save_breakpoint(deps: &Deps, args: &Value) -> Result<Value, String> {
    let mut raw = args.get("breakpoint").cloned().ok_or("missing breakpoint")?;
    if raw.get("id").and_then(|v| v.as_str()).map(|s| s.is_empty()).unwrap_or(true) {
        raw["id"] = json!(super::gen_id());
    }
    if raw.get("enabled").is_none() {
        raw["enabled"] = json!(true);
    }
    let bp: crate::breakpoints::Breakpoint =
        serde_json::from_value(raw).map_err(|e| format!("bad breakpoint: {e}"))?;
    let bps = crate::breakpoints::upsert_breakpoint(&deps.rules_dir, bp)?;
    *deps.state.breakpoints.write().unwrap() = bps.clone();
    Ok(json!({ "breakpoints": bps }))
}

fn tool_delete_breakpoint(deps: &Deps, args: &Value) -> Result<Value, String> {
    let id = str_arg(args, "id").ok_or("missing id")?;
    let bps = crate::breakpoints::remove_breakpoint(&deps.rules_dir, &id)?;
    *deps.state.breakpoints.write().unwrap() = bps.clone();
    Ok(json!({ "breakpoints": bps }))
}

fn tool_list_paused(deps: &Deps, args: &Value) -> Result<Value, String> {
    let max = u64_arg(args, "maxBodyBytes").unwrap_or(50_000) as usize;
    let paused: Vec<Value> = deps
        .state
        .store
        .all()
        .iter()
        .filter(|f| f.state == FlowState::Paused)
        .map(|f| flow_to_json(f, max))
        .collect();
    Ok(json!({ "paused": paused }))
}

fn tool_resolve_breakpoint(deps: &Deps, args: &Value) -> Result<Value, String> {
    let id = u64_arg(args, "flowId").ok_or("missing flowId")?;
    let phase = str_arg(args, "phase").ok_or("missing phase")?;
    let action = str_arg(args, "action").ok_or("missing action")?;
    let edited: crate::commands::EditedPayload =
        serde_json::from_value(args.get("edits").cloned().unwrap_or_else(|| json!({})))
            .map_err(|e| format!("bad edits: {e}"))?;
    crate::commands::resolve_breakpoint_core(&deps.state.pending_breakpoints, id, &phase, &action, edited)?;
    Ok(json!({ "ok": true }))
}

fn tool_send_request(_deps: &Deps, args: &Value) -> Result<Value, String> {
    let req: crate::httpsend::SendRequest =
        serde_json::from_value(args.clone()).map_err(|e| format!("bad request: {e}"))?;
    let via_proxy = args.get("viaProxy").and_then(|v| v.as_bool()).unwrap_or(false);
    let max = u64_arg(args, "maxBodyBytes").unwrap_or(50_000) as usize;
    let resp = crate::httpsend::send_http(&req, via_proxy);
    let mut v = serde_json::to_value(&resp).map_err(|e| e.to_string())?;
    if let Some(b) = v.get("body").and_then(|b| b.as_str()) {
        if b.len() > max {
            let cut: String = b.chars().take(max).collect();
            v["body"] = json!(cut);
            v["truncated"] = json!(true);
        }
    }
    Ok(v)
}
```

`EditedPayload` — проверить, что struct и её поля `pub` (иначе сделать pub в commands.rs).

- [ ] **Step 4: Тесты зелёные + commit**

Run: `cd src-tauri && cargo test` — PASS.

```bash
git add src-tauri/src
git commit -m "feat(mcp): breakpoint and send-request tools"
```

---

### Task 6: Плагинный мост (Rust): реестр, pending-вызовы, команды

**Files:**
- Create: `src-tauri/src/mcp/plugin_bridge.rs`
- Modify: `src-tauri/src/mcp/mod.rs` (+`pub mod plugin_bridge;`, `McpState`, `PeerRegistry`)

**Interfaces:**
- Produces:
  - `plugin_bridge::PluginTool { plugin_id, name, description, input_schema: Value, timeout_ms: Option<u64> }`, `PluginTool::full_name() -> String` (= `{plugin_id}_{name}`)
  - `plugin_bridge::PluginBridge::{new, register, unregister, clear_plugin, find, call, resolve}`
  - Tauri-команды: `mcp_register_tool(plugin_id, name, description, input_schema, timeout_ms)`, `mcp_unregister_tool(plugin_id, name)`, `mcp_clear_plugin_tools(plugin_id)`, `mcp_tool_result(call_id, result, error)`
  - `mcp::PeerRegistry::{new, add, notify_tools_changed}` (peers: `HashMap<u64, rmcp::service::Peer<rmcp::RoleServer>>`)
  - `mcp::McpState { bridge: Arc<PluginBridge>, peers: Arc<PeerRegistry>, last_error: Mutex<Option<String>> }` + `McpState::new()` (поле server добавится в Task 7)

- [ ] **Step 1: Падающие тесты** (внизу `plugin_bridge.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tool(plugin: &str, name: &str) -> PluginTool {
        PluginTool {
            plugin_id: plugin.into(),
            name: name.into(),
            description: "d".into(),
            input_schema: json!({ "type": "object" }),
            timeout_ms: None,
        }
    }

    #[test]
    fn register_replaces_and_clear_removes() {
        let b = PluginBridge::new();
        b.register(tool("p1", "t"));
        b.register(tool("p1", "t")); // повторная регистрация замещает
        b.register(tool("p2", "t"));
        assert_eq!(b.tools.read().unwrap().len(), 2);
        assert!(b.find("p1_t").is_some());
        b.clear_plugin("p1");
        assert!(b.find("p1_t").is_none());
        assert!(b.find("p2_t").is_some());
        b.unregister("p2", "t");
        assert!(b.tools.read().unwrap().is_empty());
    }

    #[tokio::test]
    async fn call_resolves_with_result_from_webview() {
        let b = std::sync::Arc::new(PluginBridge::new());
        let t = tool("p", "echo");
        b.register(t.clone());
        let b2 = b.clone();
        let fut = b.call(
            move |payload| {
                // имитируем webview: сразу отвечаем на пришедший callId
                let call_id = payload["callId"].as_u64().unwrap();
                b2.resolve(call_id, Ok(json!({ "echo": payload["args"] })));
            },
            &t,
            json!({ "x": 1 }),
        );
        let out = fut.await.unwrap();
        assert_eq!(out["echo"]["x"], json!(1));
    }

    #[tokio::test]
    async fn call_times_out() {
        let b = PluginBridge::new();
        let mut t = tool("p", "slow");
        t.timeout_ms = Some(50);
        b.register(t.clone());
        let err = b.call(|_| {}, &t, json!({})).await.unwrap_err();
        assert!(err.contains("timed out"), "err was: {err}");
    }
}
```

- [ ] **Step 2: FAIL**

Run: `cd src-tauri && cargo test mcp::plugin_bridge` — FAIL (модуля нет).

- [ ] **Step 3: Реализация**

`src-tauri/src/mcp/plugin_bridge.rs`:

```rust
//! Мост плагинных MCP-тулов: реестр метаданных (Rust) + вызовы JS-handler-ов
//! через Tauri-событие `mcp:tool-call` и команду-ответ `mcp_tool_result`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, RwLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::State;
use tokio::sync::oneshot;

pub const DEFAULT_TIMEOUT_MS: u64 = 60_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginTool {
    pub plugin_id: String,
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

impl PluginTool {
    pub fn full_name(&self) -> String {
        format!("{}_{}", self.plugin_id, self.name)
    }
}

#[derive(Default)]
pub struct PluginBridge {
    pub tools: RwLock<Vec<PluginTool>>,
    pending: Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>,
    counter: AtomicU64,
}

impl PluginBridge {
    pub fn new() -> Self {
        Self::default()
    }

    /// Регистрирует тул; повторная регистрация того же plugin_id+name замещает.
    pub fn register(&self, tool: PluginTool) {
        let mut tools = self.tools.write().unwrap();
        tools.retain(|t| !(t.plugin_id == tool.plugin_id && t.name == tool.name));
        tools.push(tool);
    }

    pub fn unregister(&self, plugin_id: &str, name: &str) {
        self.tools
            .write()
            .unwrap()
            .retain(|t| !(t.plugin_id == plugin_id && t.name == name));
    }

    pub fn clear_plugin(&self, plugin_id: &str) {
        self.tools.write().unwrap().retain(|t| t.plugin_id != plugin_id);
    }

    pub fn find(&self, full_name: &str) -> Option<PluginTool> {
        self.tools.read().unwrap().iter().find(|t| t.full_name() == full_name).cloned()
    }

    /// Вызов плагинного тула: `emit` доставляет payload в webview, ответ
    /// приходит через `resolve` (команда mcp_tool_result). Таймаут — ошибка.
    pub async fn call(
        &self,
        emit: impl Fn(Value),
        tool: &PluginTool,
        args: Value,
    ) -> Result<Value, String> {
        let call_id = self.counter.fetch_add(1, Ordering::SeqCst) + 1;
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(call_id, tx);
        emit(json!({ "callId": call_id, "tool": tool.full_name(), "args": args }));
        let timeout = Duration::from_millis(tool.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS));
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(res)) => res,
            Ok(Err(_)) => Err(format!("plugin “{}” dropped the call", tool.plugin_id)),
            Err(_) => {
                self.pending.lock().unwrap().remove(&call_id);
                Err(format!("plugin tool “{}” timed out", tool.full_name()))
            }
        }
    }

    pub fn resolve(&self, call_id: u64, result: Result<Value, String>) {
        if let Some(tx) = self.pending.lock().unwrap().remove(&call_id) {
            let _ = tx.send(result);
        }
    }
}

// ── Tauri-команды (вызываются фронтовым мостом) ──

#[tauri::command]
pub fn mcp_register_tool(
    plugin_id: String,
    name: String,
    description: String,
    input_schema: Value,
    timeout_ms: Option<u64>,
    state: State<'_, super::McpState>,
) -> Result<(), String> {
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err("tool name must be [a-zA-Z0-9_-]".into());
    }
    state.bridge.register(PluginTool { plugin_id, name, description, input_schema, timeout_ms });
    state.peers.notify_tools_changed();
    Ok(())
}

#[tauri::command]
pub fn mcp_unregister_tool(plugin_id: String, name: String, state: State<'_, super::McpState>) {
    state.bridge.unregister(&plugin_id, &name);
    state.peers.notify_tools_changed();
}

#[tauri::command]
pub fn mcp_clear_plugin_tools(plugin_id: String, state: State<'_, super::McpState>) {
    state.bridge.clear_plugin(&plugin_id);
    state.peers.notify_tools_changed();
}

#[tauri::command]
pub fn mcp_tool_result(
    call_id: u64,
    result: Option<Value>,
    error: Option<String>,
    state: State<'_, super::McpState>,
) {
    let res = match error {
        Some(e) => Err(e),
        None => Ok(result.unwrap_or(Value::Null)),
    };
    state.bridge.resolve(call_id, res);
}
```

В `src-tauri/src/mcp/mod.rs` добавить:

```rust
pub mod plugin_bridge;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use rmcp::service::Peer;
use rmcp::RoleServer;

/// Подключённые MCP-клиенты — для notifications/tools/list_changed.
pub struct PeerRegistry {
    peers: Mutex<HashMap<u64, Peer<RoleServer>>>,
    counter: AtomicU64,
}

impl PeerRegistry {
    pub fn new() -> Self {
        PeerRegistry { peers: Mutex::new(HashMap::new()), counter: AtomicU64::new(0) }
    }

    pub fn add(&self, peer: Peer<RoleServer>) {
        let id = self.counter.fetch_add(1, Ordering::SeqCst);
        self.peers.lock().unwrap().insert(id, peer);
    }

    /// Шлёт tools/list_changed всем живым пирам; мёртвые выбрасывает.
    /// Пустой реестр — no-op (важно для тестов без async-runtime).
    pub fn notify_tools_changed(self: &Arc<Self>) {
        let snapshot: Vec<(u64, Peer<RoleServer>)> = self
            .peers
            .lock()
            .unwrap()
            .iter()
            .map(|(k, v)| (*k, v.clone()))
            .collect();
        if snapshot.is_empty() {
            return;
        }
        let reg = self.clone();
        tauri::async_runtime::spawn(async move {
            for (id, peer) in snapshot {
                if peer.notify_tool_list_changed().await.is_err() {
                    reg.peers.lock().unwrap().remove(&id);
                }
            }
        });
    }
}

pub struct McpState {
    pub bridge: Arc<plugin_bridge::PluginBridge>,
    pub peers: Arc<PeerRegistry>,
    pub last_error: Mutex<Option<String>>,
}

impl McpState {
    pub fn new() -> Self {
        McpState {
            bridge: Arc::new(plugin_bridge::PluginBridge::new()),
            peers: Arc::new(PeerRegistry::new()),
            last_error: Mutex::new(None),
        }
    }
}
```

(`state.peers` в командах — это `Arc<PeerRegistry>`, метод берётся через `self: &Arc<Self>` — вызов `state.peers.notify_tools_changed()` работает.)

- [ ] **Step 4: Тесты зелёные + commit**

Run: `cd src-tauri && cargo test` — PASS.

```bash
git add src-tauri/src
git commit -m "feat(mcp): plugin tool bridge — registry, pending calls, commands"
```

---

### Task 7: rmcp-сервер: ServerHandler, auth, lifecycle, wiring

**Files:**
- Create: `src-tauri/src/mcp/server.rs`
- Modify: `src-tauri/src/mcp/mod.rs` (+`pub mod server;`, поле `server` в McpState, `apply_config`, конфиг-команды)
- Modify: `src-tauri/src/lib.rs` (manage McpState, старт в setup, регистрация команд)

**Interfaces:**
- Consumes: `core_tools::{Deps, core_tools, dispatch}`, `plugin_bridge`, `McpConfig`, `McpState`, `PeerRegistry`, `commands::{data_dir, rules_dir, AppState}`.
- Produces:
  - `server::TrawlMcp { app: AppHandle }` — `impl rmcp::ServerHandler`
  - `server::ServerHandle { pub addr: SocketAddr }` + `ServerHandle::stop(self)`
  - `server::start_server(app: AppHandle, cfg: McpConfig) -> Result<ServerHandle, String>`
  - `server::require_bearer(token: &str, req: axum::extract::Request, next: axum::middleware::Next) -> axum::response::Response`
  - `mcp::apply_config(app: &AppHandle, cfg: &McpConfig)` (async)
  - Команды: `mcp_get_config`, `mcp_set_config(enabled, port)`, `mcp_regen_token`, `mcp_server_status`

- [ ] **Step 1: Падающий интеграционный тест** (в `server.rs` внизу)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::McpConfig;

    fn init_payload() -> serde_json::Value {
        serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": { "name": "test", "version": "0" }
            }
        })
    }

    #[tokio::test]
    async fn rejects_missing_token_accepts_valid() {
        let app = tauri::test::mock_builder()
            .manage(crate::commands::AppState::new())
            .manage(crate::mcp::McpState::new())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let cfg = McpConfig { enabled: true, port: 0, token: "secret".into() };
        let handle = start_server(app.handle().clone(), cfg).await.unwrap();
        let url = format!("http://{}/mcp", handle.addr);
        let client = reqwest::Client::new();

        let r = client
            .post(&url)
            .header("Accept", "application/json, text/event-stream")
            .json(&init_payload())
            .send()
            .await
            .unwrap();
        assert_eq!(r.status().as_u16(), 401);

        let r = client
            .post(&url)
            .header("Authorization", "Bearer secret")
            .header("Accept", "application/json, text/event-stream")
            .json(&init_payload())
            .send()
            .await
            .unwrap();
        assert!(r.status().is_success(), "status was {}", r.status());
    }
}
```

Порт 0 → ОС выдаёт свободный порт, `handle.addr` его сообщает.

- [ ] **Step 2: FAIL**

Run: `cd src-tauri && cargo test mcp::server` — FAIL (модуля нет).

- [ ] **Step 3: Реализация server.rs**

```rust
//! rmcp ServerHandler + Streamable HTTP транспорт (axum) с bearer-аутентификацией.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::Request;
use axum::http::{header::AUTHORIZATION, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use rmcp::handler::server::ServerHandler;
use rmcp::model::*;
use rmcp::service::{RequestContext, RoleServer};
use serde_json::Value;
use tauri::Manager;

use super::{core_tools, McpConfig, McpState};

#[derive(Clone)]
pub struct TrawlMcp {
    app: tauri::AppHandle,
}

fn tool_from(name: String, description: String, schema: Value) -> Tool {
    let obj = schema.as_object().cloned().unwrap_or_default();
    Tool::new(name, description, Arc::new(obj))
}

impl ServerHandler for TrawlMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .enable_tool_list_changed()
                .build(),
            server_info: Implementation {
                name: "trawl".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                ..Default::default()
            },
            instructions: Some(
                "Trawl is a MITM HTTP(S) proxy. Inspect captured traffic (query_flows/get_flow), \
                 manage rewrite rules and projects, resolve paused breakpoints, send requests. \
                 Start with get_status."
                    .into(),
            ),
            ..Default::default()
        }
    }

    async fn initialize(
        &self,
        _request: InitializeRequestParam,
        context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, ErrorData> {
        // Запоминаем пира — ему будем слать tools/list_changed.
        self.app.state::<McpState>().peers.add(context.peer.clone());
        Ok(self.get_info())
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let mut tools: Vec<Tool> = core_tools::core_tools()
            .into_iter()
            .map(|d| tool_from(d.name.to_string(), d.description.to_string(), d.schema))
            .collect();
        let mcp = self.app.state::<McpState>();
        for t in mcp.bridge.tools.read().unwrap().iter() {
            tools.push(tool_from(t.full_name(), t.description.clone(), t.input_schema.clone()));
        }
        Ok(ListToolsResult { tools, ..Default::default() })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let name = request.name.to_string();
        let args = Value::Object(request.arguments.unwrap_or_default());
        let mcp = self.app.state::<McpState>();
        let result = if let Some(tool) = mcp.bridge.find(&name) {
            let app = self.app.clone();
            let bridge = mcp.bridge.clone();
            bridge
                .call(
                    move |payload| {
                        use tauri::Emitter;
                        let _ = app.emit("mcp:tool-call", payload);
                    },
                    &tool,
                    args,
                )
                .await
        } else {
            // Кор-тулы синхронные (rusqlite/файлы/blocking reqwest) — уводим с async-потока.
            let app = self.app.clone();
            tokio::task::spawn_blocking(move || {
                let state = app.state::<crate::commands::AppState>();
                let deps = core_tools::Deps {
                    state: state.inner(),
                    data_dir: crate::commands::data_dir(&app)?,
                    rules_dir: crate::commands::rules_dir(&app)?,
                };
                core_tools::dispatch(&deps, &name, &args)
            })
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r)
        };
        Ok(match result {
            Ok(v) => CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&v).unwrap_or_else(|_| v.to_string()),
            )]),
            Err(e) => CallToolResult::error(vec![Content::text(e)]),
        })
    }
}

// ── transport ──

pub struct ServerHandle {
    pub addr: SocketAddr,
    shutdown: tokio::sync::oneshot::Sender<()>,
}

impl ServerHandle {
    pub fn stop(self) {
        let _ = self.shutdown.send(());
    }
}

pub async fn require_bearer(token: &str, req: Request, next: Next) -> Response {
    let ok = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|t| t == token)
        .unwrap_or(false);
    if ok {
        next.run(req).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}

pub async fn start_server(app: tauri::AppHandle, cfg: McpConfig) -> Result<ServerHandle, String> {
    use rmcp::transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    };
    // Только loopback: MCP-сервер не должен быть виден с LAN.
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", cfg.port))
        .await
        .map_err(|e| format!("bind 127.0.0.1:{}: {e}", cfg.port))?;
    let addr = listener.local_addr().map_err(|e| e.to_string())?;
    let handler_app = app.clone();
    let service = StreamableHttpService::new(
        move || Ok(TrawlMcp { app: handler_app.clone() }),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default(),
    );
    let token = cfg.token.clone();
    let router = axum::Router::new().nest_service("/mcp", service).layer(
        axum::middleware::from_fn(move |req: Request, next: Next| {
            let token = token.clone();
            async move { require_bearer(&token, req, next).await }
        }),
    );
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    tauri::async_runtime::spawn(async move {
        let _ = axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = rx.await;
            })
            .await;
    });
    Ok(ServerHandle { addr, shutdown: tx })
}
```

Если `ServerCapabilities::builder()` не имеет `enable_tool_list_changed` — искать метод в docs.rs (`ServerCapabilitiesBuilder`); в rmcp он называется `enable_tool_list_changed()`. Если `ListToolsResult` не имеет Default — заменить на `ListToolsResult { tools, next_cursor: None }`.

- [ ] **Step 4: lifecycle + команды в mod.rs**

В `McpState` добавить поле `pub server: Mutex<Option<server::ServerHandle>>` (инициализация `Mutex::new(None)` в `new()`), `pub mod server;` рядом с остальными.

```rust
/// Останавливает и (если enabled) заново поднимает сервер по конфигу.
pub async fn apply_config(app: &tauri::AppHandle, cfg: &McpConfig) {
    use tauri::Manager;
    let mcp = app.state::<McpState>();
    if let Some(h) = mcp.server.lock().unwrap().take() {
        h.stop();
    }
    *mcp.last_error.lock().unwrap() = None;
    if !cfg.enabled {
        return;
    }
    match server::start_server(app.clone(), cfg.clone()).await {
        Ok(h) => *mcp.server.lock().unwrap() = Some(h),
        Err(e) => *mcp.last_error.lock().unwrap() = Some(e),
    }
}

#[tauri::command]
pub fn mcp_get_config(app: tauri::AppHandle) -> Result<McpConfig, String> {
    Ok(load_config(&crate::commands::data_dir(&app)?))
}

#[tauri::command]
pub async fn mcp_set_config(app: tauri::AppHandle, enabled: bool, port: u16) -> Result<McpConfig, String> {
    let dir = crate::commands::data_dir(&app)?;
    let mut cfg = load_config(&dir);
    cfg.enabled = enabled;
    cfg.port = port;
    save_config(&dir, &cfg)?;
    apply_config(&app, &cfg).await;
    Ok(cfg)
}

#[tauri::command]
pub async fn mcp_regen_token(app: tauri::AppHandle) -> Result<McpConfig, String> {
    let dir = crate::commands::data_dir(&app)?;
    let mut cfg = load_config(&dir);
    cfg.token = gen_token();
    save_config(&dir, &cfg)?;
    apply_config(&app, &cfg).await;
    Ok(cfg)
}

#[tauri::command]
pub fn mcp_server_status(state: tauri::State<'_, McpState>) -> serde_json::Value {
    serde_json::json!({
        "running": state.server.lock().unwrap().is_some(),
        "error": *state.last_error.lock().unwrap(),
    })
}
```

В `src-tauri/src/lib.rs`:
- после `.manage(AppState::new())` → `.manage(mcp::McpState::new())`
- в setup-хук после init_db:

```rust
match commands::data_dir(app.handle()) {
    Ok(dir) => {
        let cfg = mcp::load_config(&dir);
        let handle = app.handle().clone();
        tauri::async_runtime::spawn(async move {
            mcp::apply_config(&handle, &cfg).await;
        });
    }
    Err(e) => eprintln!("mcp: no data dir: {e}"),
}
```

- в `generate_handler![...]` добавить: `mcp::mcp_get_config, mcp::mcp_set_config, mcp::mcp_regen_token, mcp::mcp_server_status, mcp::plugin_bridge::mcp_register_tool, mcp::plugin_bridge::mcp_unregister_tool, mcp::plugin_bridge::mcp_clear_plugin_tools, mcp::plugin_bridge::mcp_tool_result,`

- [ ] **Step 5: Тесты зелёные + commit**

Run: `cd src-tauri && cargo test` — PASS (включая интеграционный).

```bash
git add src-tauri/src
git commit -m "feat(mcp): rmcp streamable-http server with bearer auth and lifecycle"
```

---

### Task 8: Фронтовый мост `__TRAWL__.mcp`

**Files:**
- Create: `src/plugins/mcpBridge.ts`
- Create: `src/plugins/mcpBridge.test.ts`
- Modify: `src/plugins/api.ts` (типы + поле `mcp` в TrawlHost, HOST_VERSION остаётся в host.ts)
- Modify: `src/plugins/host.ts` (секция `mcp`, вызов `initMcpBridge()`, `HOST_VERSION` → `"1.6.0"`)
- Modify: `src/plugins/loader.ts` (контекст загрузки + очистка тулов)
- Modify: `src/plugins.ts` (очистка тулов при disable)

**Interfaces:**
- Consumes: Tauri-команды из Task 6.
- Produces:
  - `mcpBridge.setLoadingPlugin(id: string | null): void`
  - `mcpBridge.registerTool(spec: McpToolSpec): Promise<void>` (кидает вне контекста загрузки)
  - `mcpBridge.unregisterTool(name: string): Promise<void>`
  - `mcpBridge.clearPluginTools(pluginId: string): Promise<void>`
  - `mcpBridge.handleToolCall(e: { callId: number; tool: string; args: unknown }): Promise<void>` (экспорт для тестов)
  - `mcpBridge.initMcpBridge(): void`
  - `api.McpToolSpec { name; description; inputSchema: Record<string, unknown>; handler: (args: unknown) => unknown | Promise<unknown>; timeoutMs?: number }`
  - `TrawlHost.mcp: { registerTool; unregisterTool }`

- [ ] **Step 1: Падающие тесты** — `src/plugins/mcpBridge.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from "vitest";

const invoke = vi.fn().mockResolvedValue(undefined);
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

import {
  registerTool,
  setLoadingPlugin,
  clearPluginTools,
  handleToolCall,
} from "./mcpBridge";

describe("mcpBridge", () => {
  beforeEach(() => {
    invoke.mockClear();
    setLoadingPlugin(null);
  });

  it("rejects registerTool outside plugin initialization", async () => {
    await expect(
      registerTool({ name: "t", description: "", inputSchema: {}, handler: () => null }),
    ).rejects.toThrow(/initialization/);
  });

  it("registers with the loading plugin id and dispatches calls", async () => {
    setLoadingPlugin("my-plugin");
    await registerTool({
      name: "echo",
      description: "d",
      inputSchema: { type: "object" },
      handler: (args) => ({ got: args }),
    });
    setLoadingPlugin(null);
    expect(invoke).toHaveBeenCalledWith(
      "mcp_register_tool",
      expect.objectContaining({ pluginId: "my-plugin", name: "echo" }),
    );

    await handleToolCall({ callId: 5, tool: "my-plugin_echo", args: { x: 1 } });
    expect(invoke).toHaveBeenCalledWith("mcp_tool_result", {
      callId: 5,
      result: { got: { x: 1 } },
      error: null,
    });
  });

  it("reports handler errors", async () => {
    setLoadingPlugin("p");
    await registerTool({
      name: "boom",
      description: "",
      inputSchema: {},
      handler: () => {
        throw new Error("nope");
      },
    });
    setLoadingPlugin(null);
    await handleToolCall({ callId: 1, tool: "p_boom", args: {} });
    expect(invoke).toHaveBeenCalledWith("mcp_tool_result", {
      callId: 1,
      result: null,
      error: expect.stringContaining("nope"),
    });
  });

  it("clearPluginTools drops handlers and reports missing handler", async () => {
    setLoadingPlugin("p2");
    await registerTool({ name: "t", description: "", inputSchema: {}, handler: () => 1 });
    setLoadingPlugin(null);
    await clearPluginTools("p2");
    expect(invoke).toHaveBeenCalledWith("mcp_clear_plugin_tools", { pluginId: "p2" });
    await handleToolCall({ callId: 2, tool: "p2_t", args: {} });
    expect(invoke).toHaveBeenCalledWith("mcp_tool_result", {
      callId: 2,
      result: null,
      error: expect.stringContaining("no handler"),
    });
  });
});
```

Run: `pnpm test` — FAIL (модуля нет).

- [ ] **Step 2: Реализация** — `src/plugins/mcpBridge.ts`:

```ts
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { McpToolSpec } from "./api";

/** Плагин, чей бандл выполняется прямо сейчас (ставится loader-ом). Регистрация
 *  MCP-тулов разрешена только в этот момент — так тул атрибуцируется плагину. */
let loadingPluginId: string | null = null;
const handlers = new Map<string, McpToolSpec["handler"]>(); // key: `${pluginId}_${name}`

export function setLoadingPlugin(id: string | null): void {
  loadingPluginId = id;
}

export async function registerTool(spec: McpToolSpec): Promise<void> {
  if (!loadingPluginId) {
    throw new Error("mcp.registerTool must be called during plugin initialization");
  }
  const pluginId = loadingPluginId;
  handlers.set(`${pluginId}_${spec.name}`, spec.handler);
  await invoke("mcp_register_tool", {
    pluginId,
    name: spec.name,
    description: spec.description,
    inputSchema: spec.inputSchema,
    timeoutMs: spec.timeoutMs ?? null,
  });
}

export async function unregisterTool(name: string): Promise<void> {
  if (!loadingPluginId) {
    throw new Error("mcp.unregisterTool must be called during plugin initialization");
  }
  handlers.delete(`${loadingPluginId}_${name}`);
  await invoke("mcp_unregister_tool", { pluginId: loadingPluginId, name });
}

/** Снять все тулы плагина (перед перезагрузкой бандла и при disable). */
export async function clearPluginTools(pluginId: string): Promise<void> {
  for (const key of [...handlers.keys()]) {
    if (key.startsWith(`${pluginId}_`)) handlers.delete(key);
  }
  await invoke("mcp_clear_plugin_tools", { pluginId });
}

type ToolCallEvent = { callId: number; tool: string; args: unknown };

export async function handleToolCall(e: ToolCallEvent): Promise<void> {
  const handler = handlers.get(e.tool);
  if (!handler) {
    await invoke("mcp_tool_result", {
      callId: e.callId,
      result: null,
      error: `no handler for ${e.tool}`,
    });
    return;
  }
  try {
    const result = await handler(e.args);
    await invoke("mcp_tool_result", { callId: e.callId, result: result ?? null, error: null });
  } catch (err) {
    await invoke("mcp_tool_result", { callId: e.callId, result: null, error: String(err) });
  }
}

let listening = false;

export function initMcpBridge(): void {
  if (listening) return;
  listening = true;
  void listen<ToolCallEvent>("mcp:tool-call", (e) => void handleToolCall(e.payload));
}
```

- [ ] **Step 3: Типы и host**

`src/plugins/api.ts` — добавить:

```ts
export interface McpToolSpec {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
  handler: (args: unknown) => unknown | Promise<unknown>;
  /** Таймаут вызова, мс (по умолчанию 60000). */
  timeoutMs?: number;
}

export interface TrawlMcp {
  /** Зарегистрировать MCP-тул `<pluginId>_<name>`. Только при инициализации плагина. */
  registerTool(spec: McpToolSpec): Promise<void>;
  unregisterTool(name: string): Promise<void>;
}
```

и в `TrawlHost` поле `mcp: TrawlMcp;`.

`src/plugins/host.ts`: импорт `{ initMcpBridge, registerTool, unregisterTool } from "./mcpBridge"`; в объект host добавить `mcp: { registerTool, unregisterTool },`; `HOST_VERSION` поднять до `"1.6.0"`; в конце `installHost()` (после `window.__TRAWL__ = host;`) вызвать `initMcpBridge();`.

`src/plugins/loader.ts` — в `loadBundle`:

```ts
import { clearPluginTools, setLoadingPlugin } from "./mcpBridge";
// в начале loadBundle, до инъекции:
await clearPluginTools(id);
// вокруг Promise с инъекцией скрипта:
setLoadingPlugin(id);
try {
  await new Promise<void>((resolve, reject) => {
    /* ...существующий код инъекции без изменений... */
  });
} finally {
  setLoadingPlugin(null);
}
```

`src/plugins.ts` — в action `setEnabled` после успешного invoke, при `enabled === false`:

```ts
if (!enabled) {
  const { clearPluginTools } = await import("./plugins/mcpBridge");
  await clearPluginTools(id);
}
```

- [ ] **Step 4: Тесты зелёные + commit**

Run: `pnpm test` — PASS (все, включая старые). Также `pnpm build` (tsc) — без ошибок типов.

```bash
git add src/plugins src/plugins.ts
git commit -m "feat(mcp): frontend bridge — __TRAWL__.mcp plugin tools"
```

---

### Task 9: UI-блок MCP в SetupPanel

**Files:**
- Create: `src/mcp.ts`
- Create: `src/mcp.test.ts`
- Create: `src/components/McpSection.tsx`
- Modify: `src/components/SetupPanel.tsx` (отрендерить `<McpSection />` в конце панели)

**Interfaces:**
- Consumes: команды `mcp_get_config` / `mcp_set_config` / `mcp_regen_token` / `mcp_server_status`, компонент `CopyableCommand({ cmd })`.
- Produces: `mcp.getMcpConfig/setMcpConfig/regenMcpToken/mcpServerStatus`, `mcp.mcpAddCommand(port, token): string`.

- [ ] **Step 1: Падающий тест** — `src/mcp.test.ts`:

```ts
import { describe, it, expect, vi } from "vitest";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
import { mcpAddCommand } from "./mcp";

describe("mcpAddCommand", () => {
  it("builds the claude mcp add command", () => {
    expect(mcpAddCommand(9910, "abc")).toBe(
      'claude mcp add --transport http trawl http://127.0.0.1:9910/mcp --header "Authorization: Bearer abc"',
    );
  });
});
```

Run: `pnpm test mcp` — FAIL.

- [ ] **Step 2: `src/mcp.ts`**

```ts
import { invoke } from "@tauri-apps/api/core";

export interface McpConfig {
  enabled: boolean;
  port: number;
  token: string;
}

export interface McpServerStatus {
  running: boolean;
  error: string | null;
}

export const getMcpConfig = () => invoke<McpConfig>("mcp_get_config");
export const setMcpConfig = (enabled: boolean, port: number) =>
  invoke<McpConfig>("mcp_set_config", { enabled, port });
export const regenMcpToken = () => invoke<McpConfig>("mcp_regen_token");
export const mcpServerStatus = () => invoke<McpServerStatus>("mcp_server_status");

export function mcpAddCommand(port: number, token: string): string {
  return `claude mcp add --transport http trawl http://127.0.0.1:${port}/mcp --header "Authorization: Bearer ${token}"`;
}
```

- [ ] **Step 3: `src/components/McpSection.tsx`**

Стилистику взять из существующих секций SetupPanel (посмотреть соседние блоки и классы перед вёрсткой; PulseDot — индикатор):

```tsx
import { useEffect, useState } from "react";
import { RefreshCw } from "lucide-react";
import {
  getMcpConfig,
  mcpAddCommand,
  mcpServerStatus,
  regenMcpToken,
  setMcpConfig,
  type McpConfig,
  type McpServerStatus,
} from "../mcp";
import { CopyableCommand } from "./CopyableCommand";
import { PulseDot } from "./PulseDot";

export function McpSection() {
  const [cfg, setCfg] = useState<McpConfig | null>(null);
  const [status, setStatus] = useState<McpServerStatus | null>(null);
  const [port, setPort] = useState("");

  const refresh = async () => {
    const c = await getMcpConfig();
    setCfg(c);
    setPort(String(c.port));
    setStatus(await mcpServerStatus());
  };

  useEffect(() => {
    void refresh();
  }, []);

  if (!cfg) return null;

  const apply = async (enabled: boolean, p: number) => {
    setCfg(await setMcpConfig(enabled, p));
    setStatus(await mcpServerStatus());
  };

  return (
    <section className="space-y-2">
      <div className="flex items-center gap-2">
        <h3 className="font-medium">MCP server</h3>
        <PulseDot active={Boolean(status?.running)} />
        <label className="ml-auto flex items-center gap-1.5 text-sm">
          <input
            type="checkbox"
            checked={cfg.enabled}
            onChange={(e) => void apply(e.target.checked, cfg.port)}
          />
          Enabled
        </label>
      </div>
      {status?.error && <p className="text-sm text-red-500">{status.error}</p>}
      <div className="flex items-center gap-2 text-sm">
        <span>Port</span>
        <input
          className="w-20 rounded border border-border bg-background px-1.5 py-0.5"
          value={port}
          onChange={(e) => setPort(e.target.value)}
          onBlur={() => {
            const p = Number(port);
            if (Number.isInteger(p) && p > 0 && p < 65536 && p !== cfg.port) {
              void apply(cfg.enabled, p);
            } else {
              setPort(String(cfg.port));
            }
          }}
        />
        <button
          className="ml-auto flex items-center gap-1 rounded border border-border px-2 py-0.5 hover:bg-secondary"
          onClick={async () => {
            setCfg(await regenMcpToken());
            setStatus(await mcpServerStatus());
          }}
        >
          <RefreshCw className="size-3" /> Regenerate token
        </button>
      </div>
      <p className="text-sm text-muted-foreground">
        Connect an AI agent (Claude Code, Cursor…) to inspect traffic and manage rules:
      </p>
      <CopyableCommand cmd={mcpAddCommand(cfg.port, cfg.token)} />
    </section>
  );
}
```

`PulseDot` — сверить пропсы по `src/components/PulseDot.tsx` перед использованием (если у него иной проп, адаптировать вызов).

В `SetupPanel.tsx`: импорт и рендер `<McpSection />` последней секцией панели (после существующих шагов).

- [ ] **Step 4: Тесты + сборка + commit**

Run: `pnpm test` — PASS. `pnpm build` — без ошибок.

```bash
git add src/mcp.ts src/mcp.test.ts src/components/McpSection.tsx src/components/SetupPanel.tsx
git commit -m "feat(mcp): setup panel section — toggle, port, token, connect command"
```

---

### Task 10: Документация + финальная проверка

**Files:**
- Modify: `docs/plugins.md`
- Modify: `README.md` (короткий раздел «MCP server»)

- [ ] **Step 1: docs/plugins.md**

После раздела про `registerFlowAction`/host API добавить раздел:

```markdown
## MCP tools

Trawl runs a local MCP server (Setup → MCP server) that AI agents connect to.
A plugin can contribute its own tools:

```ts
const host = window.__TRAWL__;

host.mcp.registerTool({
  name: "flaky_endpoints",              // exposed as "<pluginId>_flaky_endpoints"
  description: "Endpoints with the highest error rate.",
  inputSchema: {
    type: "object",
    properties: { limit: { type: "integer" } },
  },
  async handler({ limit = 10 }) {
    const buckets = await host.flows.aggregate({ statusClass: "5xx" }, "host", 0, limit);
    return { buckets };                 // any JSON-serializable value
  },
  timeoutMs: 30_000,                    // optional, default 60000
});
```

Rules:

- `registerTool` must be called **during plugin initialization** (top level of
  your bundle) — that is how the tool is attributed to your plugin.
- Tool names: `[a-zA-Z0-9_-]`. The final MCP name is `<pluginId>_<name>`.
- The handler's return value is serialized to JSON for the agent; thrown
  errors become tool errors.
- Tools are removed automatically when the plugin is disabled or reloaded.

Requires host API `1.6.0`.
```

(Вложенный код-блок оформить так, как в файле уже делаются вложенные примеры; если тройные бэктики конфликтуют — использовать отступы.)

- [ ] **Step 2: README.md**

Добавить в список фич строку-абзац:

```markdown
## MCP server

Trawl embeds an MCP server (Streamable HTTP, `127.0.0.1:9910`, bearer token) so
AI agents can inspect captured traffic, manage rewrite rules, projects and
breakpoints, and resolve paused requests. Grab the ready-made
`claude mcp add …` command in **Setup → MCP server**. Plugins can contribute
their own tools via `__TRAWL__.mcp.registerTool` (see docs/plugins.md).
```

- [ ] **Step 3: Полный прогон**

Run: `cd src-tauri && cargo test && cd .. && pnpm test && pnpm build` — всё зелёное.

- [ ] **Step 4: Ручная проверка (чек-лист)**

1. `pnpm tauri dev` — приложение стартует, в Setup появился блок «MCP server», статус зелёный.
2. Скопировать команду, выполнить `claude mcp add …` и в Claude Code: `/mcp` → сервер trawl подключён, в списке ~19 тулов.
3. Попросить агента: `get_status`, затем `query_flows` (запустив прокси и прогнав пару запросов) — данные совпадают с UI.
4. Попросить агента создать правило (например, подмена заголовка) — правило появилось в UI, конфликт двух правил возвращает внятную ошибку.
5. Создать брейкпоинт через агента, поймать запрос, `list_paused` → `resolve_breakpoint` с правкой — запрос ушёл изменённым.
6. `curl -X POST http://127.0.0.1:9910/mcp` без токена → 401.
7. Выключить тоггл — сервер погас (`/mcp` в Claude Code теряет соединение), включить — поднялся.
8. Тестовый плагин с `mcp.registerTool` — тул виден в `/mcp` списке, вызывается, после disable плагина исчезает (list_changed).

- [ ] **Step 5: Commit**

```bash
git add docs/plugins.md README.md
git commit -m "docs: MCP server — usage and plugin tools"
```

---

## Self-Review (выполнен при написании плана)

- Покрытие спеки: транспорт+auth (T1, T7), кор-тулы все 19 (T3–T5), плагинные тулы+list_changed (T6–T8), UI (T9), доки (T10), тесты — в каждой задаче. Push-нотификации брейкпоинтов и stdio — вне объёма по спеке.
- Типы согласованы: `Deps`/`dispatch`/`flow_to_json` (T3) используются в T4/T5/T7; `PluginBridge.call(emit, tool, args)` (T6) совпадает с вызовом в T7; команды T6/T7 совпадают с invoke-вызовами T8/T9.
- Известные точки адаптации под rmcp 2.2 указаны прямо в шагах (builder-методы capabilities, `ListToolsResult` Default, методы записи в БД в тесте T3) — это проверка на месте, а не TBD.
