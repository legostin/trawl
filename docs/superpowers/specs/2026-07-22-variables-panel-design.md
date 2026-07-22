# Variables panel (global + per-project env) — design

Date: 2026-07-22
Status: approved

## Overview

Today environment variables exist only inside a project (`Project.env`) and are
edited one project at a time in the Project Editor modal. Scripts read/write
`env.KEY` of the active project only; rules expand `{{KEY}}` from the active
project only. There is no project-less (global) env: with no active project,
`env` is empty and script writes are lost.

This feature adds **global variables** and a **Variables panel** that manages
all variables in one place — the global scope plus every project's scope.

Secrets (Keychain-backed) are a separate, existing concept and are not touched.

## 1. Data model

- `ProjectsFile` (Rust and TS) gains `globalEnv: Vec<EnvVar>` with
  `#[serde(default)]` — old `projects.json` files load without migration.
- **Effective env** = `globalEnv` overlaid with the active project's env; on a
  key collision the project value wins.

## 2. Backend (Rust)

- `projects.rs`:
  - `global_env` field on `ProjectsFile`;
  - `merged_env_object(global, project)` — builds the effective env JSON object;
  - `update_global_env(dir, env)` — persists the global list.
- `commands.rs`: `AppState` gains a shared `global_env` next to
  `active_project`; loaded at startup, refreshed on every save. New command
  `save_global_env(env) -> ProjectsFile`. `list_projects` returns `globalEnv`
  automatically (it serializes the whole file).
- `proxy.rs`: the env getter that feeds scripts and `{{KEY}}` rule expansion
  returns the **merged** object — the script/rule engines need no changes.
- **Script write-back** (decision: writes always target the project):
  - The script returns the modified merged object `R`. With an active project,
    the project's new env = keys of `R` that were already in the project OR
    whose value differs from the global value. Unchanged global keys are NOT
    copied into the project; a global key modified by a script becomes a
    project-level override.
  - A key deleted by the script (present in the project env but absent from
    `R`) is removed from the project. Global keys cannot be deleted by scripts.
  - `globalEnv` is never modified by scripts while a project is active.
  - With no active project, `R` is written to `globalEnv` (today such writes
    are silently lost).

## 3. Frontend

- New `VariablesPanel.tsx` — a modal shaped like the Project Editor:
  - left: scope list — **Global** pinned first, then all projects, the active
    one marked with ●;
  - right: editable key/value table for the selected scope;
  - changes persist automatically (on blur / row add / row remove), no Save
    button;
  - in a project scope, a key that also exists in Global shows an
    "overrides Global" badge.
- `EnvList` is extracted from `ProjectEditor.tsx` into a shared component and
  reused by both the Project Editor (unchanged behavior) and the panel.
- `projects.ts` (zustand store): `globalEnv` in state, `saveGlobalEnv` action,
  `variablesOpen` flag with open/close actions.
- Entry point: a TopBar button (lucide `Variable` icon) next to the Projects
  button; `<VariablesPanel />` rendered in `AppShell`.

## 4. In-code documentation

- `scripting/apiTypes.ts`: update the `env` doc comment — it is now merged
  global + project; writes go to the project (or to global when no project is
  active).
- If `HintsPanel` / autocomplete surface active-project env keys, switch them
  to the merged env.

## 5. Error handling

- Command failures surface through the existing toast pattern (same as
  `upsert`).
- Malformed `projects.json` behaves as today — `serde(default)` covers the
  missing-field case only.

## 6. Testing

- Rust: merge semantics (project wins on collision); write-back split (an
  untouched global key does not leak into the project; a script-modified
  global key becomes a project override); no-project write-back lands in
  `globalEnv`; serde backward compatibility (file without `globalEnv`).
- TS: vitest for any new pure helpers, following the existing `*.test.ts`
  pattern.
