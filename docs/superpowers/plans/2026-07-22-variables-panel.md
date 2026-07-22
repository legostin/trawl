# Variables Panel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Global (project-less) env variables plus a Variables panel managing every scope (Global + each project) in one modal.

**Architecture:** `ProjectsFile` gains `globalEnv` (serde default, no migration). The proxy's env getter returns global merged with the active project's env (project wins); script write-back targets the project (a modified global key becomes a project override), or `globalEnv` when no project is active. Frontend: a new `VariablesPanel` modal (scope list left, KV table right), `EnvList` extracted from `ProjectEditor` and shared.

**Tech Stack:** Rust (Tauri 2, serde), React + TypeScript + zustand, vitest, cargo test.

**Spec:** `docs/superpowers/specs/2026-07-22-variables-panel-design.md`

## Global Constraints

- Secrets (Keychain) are a separate feature — do not touch `secrets.rs` or its UI.
- `globalEnv` is never modified by scripts while a project is active.
- Old `projects.json` without `globalEnv` must load unchanged (`#[serde(default)]`).
- All JSON field names are camelCase (`globalEnv`) via existing `#[serde(rename_all = "camelCase")]`.
- Comments in Rust follow the file's existing style (Russian one-liners are the norm in `projects.rs`/`proxy.rs`).
- Run Rust tests from `src-tauri/`: `cargo test`. Frontend: `pnpm test` (vitest), typecheck via `pnpm build` (runs `tsc`).

---

### Task 1: `globalEnv` field + merge helpers (backend)

**Files:**
- Modify: `src-tauri/src/projects.rs`

**Interfaces:**
- Produces: `ProjectsFile.global_env: Vec<EnvVar>`; `pub fn env_list_object(env: &[EnvVar]) -> serde_json::Value`; `pub fn merged_env_object(global: &[EnvVar], project: Option<&Project>) -> serde_json::Value`; `pub fn update_global_env(dir: &Path, env: Vec<EnvVar>) -> Result<()>`.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `src-tauri/src/projects.rs`:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test merged_env_project_wins projects_file_without_global_env update_global_env_persists 2>&1 | tail -20`
Expected: compile errors — `global_env` field and `merged_env_object` / `update_global_env` not defined.

- [ ] **Step 3: Implement**

In `src-tauri/src/projects.rs`, add the field to `ProjectsFile` (line ~93):

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectsFile {
    pub projects: Vec<Project>,
    pub active_id: Option<String>,
    /// Глобальные переменные (вне проектов); проектные перекрывают их при merge.
    #[serde(default)]
    pub global_env: Vec<EnvVar>,
}
```

Add helpers (below `env_from_object`, ~line 79) and refactor `Project::env_object` to delegate:

```rust
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

/// Обновляет глобальный env на диске.
pub fn update_global_env(dir: &Path, env: Vec<EnvVar>) -> Result<()> {
    let mut file = load_projects(dir)?;
    file.global_env = env;
    save_projects(dir, &file)
}
```

`Project::env_object` becomes:

```rust
    /// env как JSON-объект для инъекции в скрипт (`env.KEY`).
    pub fn env_object(&self) -> serde_json::Value {
        env_list_object(&self.env)
    }
```

Fix the two existing struct literals in tests (lines ~177 and ~219) by adding `global_env: vec![]`:

```rust
        let file = ProjectsFile { projects: vec![proj(&["x"], &[])], active_id: Some("p1".into()), global_env: vec![] };
```

and

```rust
        let file = ProjectsFile {
            projects: vec![proj(&["example.com"], &[])],
            active_id: Some("p1".into()),
            global_env: vec![],
        };
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test projects:: 2>&1 | tail -20`
Expected: all `projects::tests` PASS (including the three new ones).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/projects.rs
git commit -m "feat(env): globalEnv in projects.json + merge helpers"
```

---

### Task 2: script write-back split (pure function)

**Files:**
- Modify: `src-tauri/src/projects.rs`

**Interfaces:**
- Consumes: `env_list_object` from Task 1.
- Produces: `pub fn split_env_writeback(returned: &serde_json::Value, project_env: &[EnvVar], global: &[EnvVar]) -> Vec<EnvVar>` — the project's new env after a script modified the merged object.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `src-tauri/src/projects.rs`:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test writeback 2>&1 | tail -10`
Expected: compile error — `split_env_writeback` not defined.

- [ ] **Step 3: Implement**

Add below `merged_env_object` in `src-tauri/src/projects.rs`:

```rust
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test writeback 2>&1 | tail -10`
Expected: 4 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/projects.rs
git commit -m "feat(env): split_env_writeback — script writes target the project"
```

---

### Task 3: wire merged env into proxy, AppState, and `save_global_env` command

**Files:**
- Modify: `src-tauri/src/proxy.rs` (handler struct ~line 70, `active_env`/`apply_env` ~lines 250–274, `start` ~line 1211, test call sites ~lines 1391–1750)
- Modify: `src-tauri/src/commands.rs` (AppState ~line 16, `start_proxy` ~lines 130–150, projects section ~line 400)
- Modify: `src-tauri/src/lib.rs` (command registration ~line 69)

**Interfaces:**
- Consumes: `merged_env_object`, `split_env_writeback`, `update_global_env`, `env_from_object` from Tasks 1–2.
- Produces: `pub type SharedGlobalEnv = Arc<RwLock<Vec<EnvVar>>>` (in `proxy.rs`); `AppState.global_env: SharedGlobalEnv`; Tauri command `save_global_env(env: Vec<EnvVar>) -> ProjectsFile` (frontend invokes `"save_global_env"` with `{ env }`).

- [ ] **Step 1: Add the shared type and handler field**

In `src-tauri/src/proxy.rs`, next to `pub type SharedProject` (~line 33):

```rust
pub type SharedGlobalEnv = Arc<RwLock<Vec<crate::projects::EnvVar>>>;
```

In `struct CaptureHandler` (~line 81), after `active_project: SharedProject,`:

```rust
    global_env: SharedGlobalEnv,
```

In `pub async fn start(...)` (~line 1211), add the parameter after `active_project: SharedProject,`:

```rust
    global_env: SharedGlobalEnv,
```

and in the `CaptureHandler { ... }` literal (~line 1237), after `active_project,`:

```rust
        global_env,
```

- [ ] **Step 2: Merge on read, split on write**

Update the import at `src-tauri/src/proxy.rs:23`:

```rust
use crate::projects::{env_from_object, merged_env_object, split_env_writeback, update_global_env, update_project_env, Project};
```

Replace `active_env` and `apply_env` (~lines 249–274):

```rust
    /// Эффективный env (global + активный проект, проект побеждает) как JSON-объект.
    fn active_env(&self) -> Value {
        let global = self.global_env.read().unwrap();
        let guard = self.active_project.read().unwrap();
        merged_env_object(&global, guard.as_ref())
    }

    /// Записывает изменённый скриптом env: при активном проекте — в проект
    /// (изменённый глобальный ключ становится проектным перекрытием),
    /// без проекта — в глобальный env.
    fn apply_env(&self, new_env: &Value) {
        if *new_env == self.active_env() {
            return; // без изменений — не пишем
        }
        let global = self.global_env.read().unwrap().clone();
        let mut guard = self.active_project.write().unwrap();
        if let Some(proj) = guard.as_ref() {
            let env = split_env_writeback(new_env, &proj.env, &global);
            let id = proj.id.clone();
            let mut updated = proj.clone();
            updated.env = env.clone();
            *guard = Some(updated);
            drop(guard);
            let _ = update_project_env(&self.data_dir, &id, env);
        } else {
            drop(guard);
            let env = env_from_object(new_env);
            *self.global_env.write().unwrap() = env.clone();
            let _ = update_global_env(&self.data_dir, env);
        }
    }
```

- [ ] **Step 3: Fix proxy tests (new `start` parameter)**

In `mod tests` of `proxy.rs`, every `start(...)` call passes `p,` for the project cell (call sites ~lines 1391, 1426, 1481, 1518, 1599, 1640 and the multiline ones ~1685, 1742). Insert a fresh empty cell right after `p,` in each:

```rust
Arc::new(RwLock::new(vec![])),
```

Example (line ~1391):

```rust
let handle = start(proxy_addr, store.clone(), emit, notify_noop(), secret_none(), ca_dir.clone(), s, r, l, p, Arc::new(RwLock::new(vec![])), ca_dir.clone(), None, bps, icept, pending, Arc::new(RwLock::new(0)), Arc::new(RwLock::new(false)))
```

- [ ] **Step 4: AppState + start_proxy + command**

In `src-tauri/src/commands.rs`, add to `AppState` (after `active_project`, ~line 24):

```rust
    /// Глобальные env-переменные (вне проектов). Разделяются с прокси-хендлером.
    pub global_env: crate::proxy::SharedGlobalEnv,
```

and to `AppState::new()` (after `active_project: ...`, ~line 47):

```rust
            global_env: Arc::new(RwLock::new(Vec::new())),
```

In `start_proxy` (~line 130), load the global env before `pfile.projects` is consumed:

```rust
    let pfile = projects::load_projects(&data_dir(&app)?).map_err(|e| e.to_string())?;
    *state.global_env.write().unwrap() = pfile.global_env.clone();
    *state.active_project.write().unwrap() = pfile
        .active_id
        .and_then(|i| pfile.projects.into_iter().find(|p| p.id == i));
```

and pass the cell to `proxy::start` — in the argument list, right after `state.active_project.clone(),`:

```rust
        state.global_env.clone(),
```

Add the command to the `// ── Проекты ──` section (after `get_active_project`, ~line 452):

```rust
#[tauri::command]
pub fn save_global_env(
    app: AppHandle,
    env: Vec<projects::EnvVar>,
    state: State<'_, AppState>,
) -> Result<ProjectsFile, String> {
    let dir = data_dir(&app)?;
    projects::update_global_env(&dir, env.clone()).map_err(|e| e.to_string())?;
    *state.global_env.write().unwrap() = env;
    projects::load_projects(&dir).map_err(|e| e.to_string())
}
```

Register it in `src-tauri/src/lib.rs` (after `commands::get_active_project,` ~line 73):

```rust
            commands::save_global_env,
```

- [ ] **Step 5: Run the full Rust test suite**

Run: `cd src-tauri && cargo test 2>&1 | tail -15`
Expected: everything compiles; all tests PASS (proxy tests included).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/proxy.rs src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat(env): merged global+project env in proxy, save_global_env command"
```

---

### Task 4: frontend store — globalEnv, saveGlobalEnv, panel flag

**Files:**
- Modify: `src/projects.ts`
- Create: `src/projects.test.ts`

**Interfaces:**
- Consumes: Tauri command `save_global_env` (Task 3) — invoked as `invoke<ProjectsFile>("save_global_env", { env })`.
- Produces (for Tasks 6–7): store fields `globalEnv: EnvVar[]`, `variablesOpen: boolean`; actions `saveGlobalEnv(env: EnvVar[]): Promise<void>`, `openVariables(): void`, `closeVariables(): void`; helper `overriddenKeys(globalEnv: EnvVar[], projectEnv: EnvVar[]): Set<string>`.

- [ ] **Step 1: Write the failing test**

Create `src/projects.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

import { overriddenKeys, useProjects } from "./projects";

describe("overriddenKeys", () => {
  it("returns project keys that shadow a global key", () => {
    const g = [
      { key: "HOST", value: "g" },
      { key: "TOKEN", value: "g" },
    ];
    const p = [
      { key: "TOKEN", value: "p" },
      { key: "LOCAL", value: "p" },
      { key: "", value: "" },
    ];
    expect(overriddenKeys(g, p)).toEqual(new Set(["TOKEN"]));
  });
});

describe("projects store — global env", () => {
  beforeEach(() => {
    invoke.mockReset();
    useProjects.setState({ projects: [], activeId: null, globalEnv: [], variablesOpen: false });
  });

  it("load() picks up globalEnv (missing field → [])", async () => {
    invoke.mockResolvedValue({ projects: [], activeId: null, globalEnv: [{ key: "G", value: "1" }] });
    await useProjects.getState().load();
    expect(useProjects.getState().globalEnv).toEqual([{ key: "G", value: "1" }]);

    invoke.mockResolvedValue({ projects: [], activeId: null });
    await useProjects.getState().load();
    expect(useProjects.getState().globalEnv).toEqual([]);
  });

  it("saveGlobalEnv invokes the command and stores the result", async () => {
    invoke.mockResolvedValue({ projects: [], activeId: null, globalEnv: [{ key: "A", value: "2" }] });
    await useProjects.getState().saveGlobalEnv([{ key: "A", value: "2" }]);
    expect(invoke).toHaveBeenCalledWith("save_global_env", { env: [{ key: "A", value: "2" }] });
    expect(useProjects.getState().globalEnv).toEqual([{ key: "A", value: "2" }]);
  });

  it("openVariables/closeVariables toggle the flag", () => {
    useProjects.getState().openVariables();
    expect(useProjects.getState().variablesOpen).toBe(true);
    useProjects.getState().closeVariables();
    expect(useProjects.getState().variablesOpen).toBe(false);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm test projects.test 2>&1 | tail -15`
Expected: FAIL — `overriddenKeys` not exported, store fields missing.

- [ ] **Step 3: Implement**

In `src/projects.ts`:

Add after the `EnvVar` interface:

```ts
/** Ключи проекта, перекрывающие одноимённые глобальные переменные. */
export function overriddenKeys(globalEnv: EnvVar[], projectEnv: EnvVar[]): Set<string> {
  const g = new Set(globalEnv.map((e) => e.key).filter(Boolean));
  return new Set(projectEnv.map((e) => e.key).filter((k) => k && g.has(k)));
}
```

Extend `ProjectsFile`:

```ts
interface ProjectsFile {
  projects: Project[];
  activeId: string | null;
  globalEnv?: EnvVar[];
}
```

Extend `ProjectsState`:

```ts
interface ProjectsState {
  projects: Project[];
  activeId: string | null;
  globalEnv: EnvVar[];
  editorOpen: boolean;
  variablesOpen: boolean;
  load: () => Promise<void>;
  setActive: (id: string | null) => Promise<void>;
  upsert: (project: Project) => Promise<void>;
  remove: (id: string) => Promise<void>;
  saveGlobalEnv: (env: EnvVar[]) => Promise<void>;
  addHost: (projectId: string, host: string) => Promise<void>;
  openEditor: () => void;
  closeEditor: () => void;
  openVariables: () => void;
  closeVariables: () => void;
}
```

Update the store implementation — initial state gets `globalEnv: [],` and `variablesOpen: false,`; `load`, `upsert`, `remove` also store `globalEnv: f.globalEnv ?? []`; new actions:

```ts
  load: async () => {
    const f = await invoke<ProjectsFile>("list_projects");
    set({ projects: f.projects, activeId: f.activeId, globalEnv: f.globalEnv ?? [] });
  },
```

(same `globalEnv: f.globalEnv ?? []` addition inside `upsert` and `remove` `set()` calls), and after `remove`:

```ts
  saveGlobalEnv: async (env) => {
    const f = await invoke<ProjectsFile>("save_global_env", { env });
    set({ projects: f.projects, activeId: f.activeId, globalEnv: f.globalEnv ?? [] });
  },
```

and after `closeEditor`:

```ts
  openVariables: () => set({ variablesOpen: true }),
  closeVariables: () => set({ variablesOpen: false }),
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `pnpm test 2>&1 | tail -10`
Expected: all vitest suites PASS (new `projects.test.ts` included).

- [ ] **Step 5: Commit**

```bash
git add src/projects.ts src/projects.test.ts
git commit -m "feat(env): globalEnv + variables panel state in projects store"
```

---

### Task 5: extract shared `EnvList` component

**Files:**
- Create: `src/components/EnvList.tsx`
- Modify: `src/components/ProjectEditor.tsx` (delete local `EnvList`, lines ~207–254; import the new one)

**Interfaces:**
- Consumes: `EnvVar` type, `overriddenKeys` (Task 4).
- Produces: `export function EnvList(props: { env: EnvVar[]; onChange: (env: EnvVar[]) => void; hint: string; overrideKeys?: Set<string> })` — used by `ProjectEditor` and `VariablesPanel` (Task 6).

- [ ] **Step 1: Create `src/components/EnvList.tsx`**

The markup is the existing `EnvList` from `ProjectEditor.tsx` with two additions: a `hint` prop (the caller supplies the text) and an optional `overrideKeys` badge:

```tsx
import { Plus, X } from "lucide-react";
import type { EnvVar } from "../projects";
import { Button } from "./ui/button";
import { Input } from "./ui/input";

export function EnvList({
  env,
  onChange,
  hint,
  overrideKeys,
}: {
  env: EnvVar[];
  onChange: (env: EnvVar[]) => void;
  hint: string;
  overrideKeys?: Set<string>;
}) {
  const setAt = (i: number, patch: Partial<EnvVar>) =>
    onChange(env.map((e, idx) => (idx === i ? { ...e, ...patch } : e)));
  return (
    <div>
      <div className="text-xs font-semibold">Environment variables</div>
      <div className="mb-1.5 text-[11px] text-muted-foreground">{hint}</div>
      <div className="space-y-1">
        {env.map((e, i) => (
          <div key={i} className="flex items-center gap-1">
            <Input
              value={e.key}
              onChange={(ev) => setAt(i, { key: ev.target.value })}
              placeholder="KEY"
              className="h-7 w-40 font-mono"
            />
            <Input
              value={e.value}
              onChange={(ev) => setAt(i, { value: ev.target.value })}
              placeholder="value"
              className="h-7 flex-1 font-mono"
            />
            {overrideKeys?.has(e.key) && (
              <span className="shrink-0 rounded bg-secondary px-1.5 py-0.5 text-[10px] text-muted-foreground">
                overrides Global
              </span>
            )}
            <Button
              size="iconSm"
              variant="ghost"
              title="Remove"
              onClick={() => onChange(env.filter((_, idx) => idx !== i))}
            >
              <X className="size-3" />
            </Button>
          </div>
        ))}
      </div>
      <Button
        size="sm"
        variant="outline"
        className="mt-1.5"
        onClick={() => onChange([...env, { key: "", value: "" }])}
      >
        <Plus />
        Add variable
      </Button>
    </div>
  );
}
```

- [ ] **Step 2: Use it in `ProjectEditor.tsx`**

Delete the local `EnvList` function (lines ~207–254). Add the import and keep behavior identical by passing the old hint text as a plain string prop. The hint contained a `<code>` element; simplify to backticks — the copy stays clear:

```tsx
import { EnvList } from "./EnvList";
```

and the call site (line ~152) becomes:

```tsx
        <EnvList
          env={draft.env}
          onChange={(env) => patch({ env })}
          hint="Available in scripts as env.KEY; scripts can also write to them (values persist across requests)."
        />
```

The `EnvVar` import in `ProjectEditor.tsx` becomes unused — remove it:

```tsx
import { useProjects, type Project } from "../projects";
```

- [ ] **Step 3: Typecheck + tests**

Run: `pnpm build 2>&1 | tail -5 && pnpm test 2>&1 | tail -5`
Expected: `tsc` clean, vite build OK, tests PASS.

- [ ] **Step 4: Commit**

```bash
git add src/components/EnvList.tsx src/components/ProjectEditor.tsx
git commit -m "refactor(ui): extract shared EnvList component"
```

---

### Task 6: VariablesPanel modal + TopBar entry

**Files:**
- Create: `src/components/VariablesPanel.tsx`
- Modify: `src/components/TopBar.tsx` (imports line 1, store hooks ~line 27, buttons ~line 79)
- Modify: `src/components/AppShell.tsx` (import ~line 21, render ~line 118)
- Modify: `src/scripting/apiTypes.ts` (env doc comment, lines 63–68)

**Interfaces:**
- Consumes: store fields/actions from Task 4 (`globalEnv`, `variablesOpen`, `saveGlobalEnv`, `openVariables`, `closeVariables`, `upsert`, `projects`, `activeId`), `EnvList` + `overriddenKeys`.
- Produces: `export function VariablesPanel()` rendered in `AppShell`.

- [ ] **Step 1: Create `src/components/VariablesPanel.tsx`**

Modal shaped like `ProjectEditor` (fixed overlay, left scope list, right table). Edits are buffered in a draft and committed on blur / row add / row remove — no Save button:

```tsx
import { useEffect, useState, type ReactNode } from "react";
import { X } from "lucide-react";
import { overriddenKeys, useProjects, type EnvVar } from "../projects";
import { EnvList } from "./EnvList";
import { cn } from "@/lib/utils";

const GLOBAL = "__global__";

export function VariablesPanel() {
  const open = useProjects((s) => s.variablesOpen);
  const close = useProjects((s) => s.closeVariables);
  const projects = useProjects((s) => s.projects);
  const activeId = useProjects((s) => s.activeId);
  const globalEnv = useProjects((s) => s.globalEnv);
  const saveGlobalEnv = useProjects((s) => s.saveGlobalEnv);
  const upsert = useProjects((s) => s.upsert);

  const [scope, setScope] = useState<string>(GLOBAL);
  const project = projects.find((p) => p.id === scope) ?? null;
  // выбранный проект удалили — откат на Global
  useEffect(() => {
    if (scope !== GLOBAL && !project) setScope(GLOBAL);
  }, [scope, project]);

  if (!open) return null;

  const env = project ? project.env : globalEnv;
  const commit = (next: EnvVar[]) => {
    if (project) void upsert({ ...project, env: next });
    else void saveGlobalEnv(next);
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-6" onClick={close}>
      <div
        className="flex h-[72vh] w-[780px] overflow-hidden rounded-lg border border-border bg-background shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex w-52 shrink-0 flex-col border-r border-border">
          <div className="flex items-center gap-2 border-b border-border bg-card px-3 py-2">
            <span className="text-xs font-semibold text-muted-foreground">Variables</span>
          </div>
          <div className="min-h-0 flex-1 overflow-auto">
            <ScopeButton selected={scope === GLOBAL} onClick={() => setScope(GLOBAL)}>
              Global
            </ScopeButton>
            {projects.map((p) => (
              <ScopeButton key={p.id} selected={p.id === scope} onClick={() => setScope(p.id)}>
                {p.name}
                {p.id === activeId && <span className="ml-1 text-http-green">●</span>}
              </ScopeButton>
            ))}
          </div>
        </div>

        <div className="min-h-0 flex-1 overflow-auto p-4">
          <ScopeEnv
            key={scope}
            env={env}
            overrideKeys={project ? overriddenKeys(globalEnv, project.env) : undefined}
            hint={
              project
                ? "Project variables. On a key clash they override Global. Changes save on blur."
                : "Available everywhere — with no active project and merged under every project (project keys win). Changes save on blur."
            }
            onCommit={commit}
          />
        </div>

        <button
          className="absolute right-8 top-8 text-muted-foreground hover:text-foreground"
          onClick={close}
          title="Close"
        >
          <X className="size-4" />
        </button>
      </div>
    </div>
  );
}

function ScopeButton({
  selected,
  onClick,
  children,
}: {
  selected: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "block w-full truncate px-3 py-2 text-left text-xs",
        selected ? "bg-primary/15" : "hover:bg-accent",
      )}
    >
      {children}
    </button>
  );
}

/** Черновик env области: типизация буферится, коммит — на blur и при +/− строки. */
function ScopeEnv({
  env,
  hint,
  overrideKeys,
  onCommit,
}: {
  env: EnvVar[];
  hint: string;
  overrideKeys?: Set<string>;
  onCommit: (env: EnvVar[]) => void;
}) {
  const [draft, setDraft] = useState(env);
  useEffect(() => setDraft(env), [env]);
  const change = (next: EnvVar[]) => {
    setDraft(next);
    if (next.length !== draft.length) onCommit(next); // добавление/удаление строки
  };
  const commitIfDirty = () => {
    if (JSON.stringify(draft) !== JSON.stringify(env)) onCommit(draft);
  };
  return (
    <div onBlur={commitIfDirty}>
      <EnvList env={draft} onChange={change} hint={hint} overrideKeys={overrideKeys} />
    </div>
  );
}
```

- [ ] **Step 2: TopBar button**

In `src/components/TopBar.tsx`: extend the lucide import (line 1):

```tsx
import { FolderCog, Moon, Play, Search, Square, Sun, Trash2, Variable } from "lucide-react";
```

add the hook next to `openEditor` (~line 27):

```tsx
  const openVariables = useProjects((s) => s.openVariables);
```

and the button right after the Projects button (~line 81):

```tsx
          <Button variant="ghost" size="iconSm" title="Variables" onClick={openVariables}>
            <Variable />
          </Button>
```

- [ ] **Step 3: Render in AppShell**

In `src/components/AppShell.tsx`, next to the `ProjectEditor` import (line 21):

```tsx
import { VariablesPanel } from "./VariablesPanel";
```

and after `<ProjectEditor />` (line 118):

```tsx
      <VariablesPanel />
```

- [ ] **Step 4: Update the `env` doc comment**

In `src/scripting/apiTypes.ts` (lines 63–68):

```ts
/**
 * Environment variables: Global merged with the active project (project wins
 * on a key clash). Read and write — written values persist to the active
 * project (with no active project — to Global) and are available to later
 * requests. Example: env.token = JSON.parse(response.body).token;
 */
declare const env: Record<string, string>;
```

- [ ] **Step 5: Typecheck, tests, manual check**

Run: `pnpm build 2>&1 | tail -5 && pnpm test 2>&1 | tail -5`
Expected: clean build, tests PASS.

Manual (optional if the reviewer wants a live check): `pnpm tauri dev`, open the panel via the TopBar `Variable` icon — add a global var, add a same-named project var, confirm the "overrides Global" badge; restart the app and confirm both persisted in `projects.json`.

- [ ] **Step 6: Commit**

```bash
git add src/components/VariablesPanel.tsx src/components/TopBar.tsx src/components/AppShell.tsx src/scripting/apiTypes.ts
git commit -m "feat(ui): Variables panel — global + per-project env in one modal"
```
