# Notifications v2 (Events, Docs, Editor UX) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add 8 new bus events with structured per-parameter docs, an event-doc panel + examples + native host controls in the notifications plugin, and hints throughout its UI.

**Architecture:** The proxy's `NotifyFn` generalizes to a named-event channel `AppEventFn` carrying `script-notify`/`rule-applied`/`rule-error`; Flow-payload events reuse `EmitFn`. The frontend host bridges everything onto the plugin bus and describes each event with `params` docs (new `EventMeta` field). The plugin renders a doc panel from `known()`, uses host Button/Input/Select, Monaco-handlebars for templates, and per-event example snippets.

**Tech Stack:** unchanged (Rust/tauri, React/TS, vitest, Monaco).

**Spec:** `docs/superpowers/specs/2026-07-22-notifications-v2-events-docs-design.md`

## Global Constraints

- Core work in an isolated **git worktree**, merged to `main` at the end; plugin in `/Users/legostin/claude-projects/trawl-plugin-notifications`, pushed to GitHub at the end.
- Versions: host API `HOST_VERSION` → **"1.7.0"**; app → **0.6.0** (package.json, src-tauri/Cargo.toml + Cargo.lock, src-tauri/tauri.conf.json); plugin → **0.2.0** with `"apiVersion": "1.7.0"` in `trawl-plugin.json` and package.json.
- New Tauri events (backend): `flow-resumed`, `breakpoint-timeout` (Flow payload via existing `EmitFn`); `rule-applied`, `rule-error` (JSON payload via `AppEventFn`). Existing `script-notify` moves onto `AppEventFn` unchanged in shape.
- Bus events + payloads exactly as the spec table defines them (`breakpoint:hit|resolved|timeout`, `rule:applied|error`, `flow:error`, `plugin:installed|removed`, `update:available`).
- `EventMeta` gains `params?: { name: string; type: string; doc?: string }[]`.
- All suites green: `cargo test` (src-tauri), `pnpm test` + `pnpm build` (both repos). No new warnings. No `git stash`.

---

## Part A — core (worktree)

### Task 1: Backend — AppEventFn + new proxy emits

**Files:**
- Modify: `src-tauri/src/proxy.rs`, `src-tauri/src/commands.rs`

**Interfaces:**
- Consumes: existing `NotifyFn`/`emit_notifications` (v1), `EmitFn`, `await_resolution`, the three rule-application sites, breakpoint resolution sites.
- Produces: `pub type AppEventFn = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>` replacing `NotifyFn` (field rename `notify` → `app_event`); Tauri events `flow-resumed`, `breakpoint-timeout`, `rule-applied`, `rule-error`.

- [ ] **Step 1: Failing tests.** In proxy.rs tests, rename the helper to `app_event_noop()` and add an integration test mirroring `rule_notify_reaches_notify_fn` (reuse its scaffolding):

```rust
    #[tokio::test]
    async fn rule_apply_and_error_reach_app_event_fn() {
        // Two rules: one that applies cleanly, one that throws.
        // rule("ok", "setHeader(request,'X-A','1');"), rule("boom", "throw new Error('kaput');")
        // Collect (event, payload) pairs through AppEventFn; run one GET through the proxy.
        // Assert: one ("rule-applied", p) with p["ruleName"]=="ok", p["phase"]=="request",
        //   p["method"]=="GET", p["flowId"].is_u64(), p["host"]/p["path"] present;
        // and one ("rule-error", p) with p["ruleName"]=="boom", p["error"] contains "kaput".
    }
```

Use the existing `rule()` test helper and upstream/proxy scaffolding; the channel is `tokio::sync::mpsc::unbounded_channel::<(String, serde_json::Value)>()`.

- [ ] **Step 2: Implement.**
  - `pub type AppEventFn = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;` replaces `NotifyFn`; `CaptureHandler.notify` → `app_event`; `start()` param renamed accordingly.
  - `emit_notifications` body: `(self.app_event)("script-notify", Value::Object(p))`.
  - New helper on `CaptureHandler`:

```rust
    /// Report a rule outcome ("rule-applied" / "rule-error") to the app.
    fn emit_rule_event(
        &self,
        event: &str,
        rule_name: &str,
        phase: &str,
        flow_id: u64,
        method: &str,
        host: &str,
        path: &str,
        error: Option<&str>,
    ) {
        let mut p = serde_json::Map::new();
        p.insert("ruleName".into(), rule_name.into());
        p.insert("phase".into(), phase.into());
        p.insert("flowId".into(), flow_id.into());
        p.insert("method".into(), method.into());
        p.insert("host".into(), host.into());
        p.insert("path".into(), path.into());
        if let Some(e) = error {
            p.insert("error".into(), e.into());
        }
        (self.app_event)(event, serde_json::Value::Object(p));
    }
```

  - Call it at the three script sites right next to the existing `emit_notifications` calls: on `"continue"|"mock"|"abort"|"breakpoint"` actions → `emit_rule_event("rule-applied", &rule.name, "request"|"response"|"handler", id, &req_method, &url.host, &url.path, None)`; on the `_ => script_error = res.error` arm → `emit_rule_event("rule-error", ..., res.error.as_deref())`. Handler phase uses `hrule.name` and phase `"handler"`.
  - `flow-resumed`: at every site where a paused flow leaves `FlowState::Paused` due to a Resolution (Execute/Respond/Abort — the blocks around proxy.rs:396-406, 637-701, and the response-phase equivalents ~914/1113): after the state change and store upsert, `(self.emit)("flow-resumed", &f)` with the updated flow. Skip the timeout branch.
  - `breakpoint-timeout`: in `await_resolution` (~line 326), the `Err(_)` branch of `tokio::time::timeout` (auto-continue): look up the flow, `(self.emit)("breakpoint-timeout", &f)`.
  - `commands.rs::start_proxy`: rename closure — `let app_event: proxy::AppEventFn = Arc::new(move |event: &str, payload| { let _ = app_for_notify.emit(event, payload); });` (note: `emit` needs a `&str` name known at runtime — `app_for_notify.emit(event, payload)` accepts it).

- [ ] **Step 3: `cargo test` all green (incl. the pre-existing `rule_notify_reaches_notify_fn` renamed/adjusted to the new type), no new warnings.** Update every test call site of `start(...)` mechanically (rename helper).

- [ ] **Step 4: Commit** `feat(proxy): rule-applied/rule-error/flow-resumed/breakpoint-timeout app events`.

---

### Task 2: Frontend — params docs, bridges, lifecycle emits, host.ui controls

**Files:**
- Modify: `src/plugins/bus.ts` (+`params` in EventMeta), `src/plugins/bus.test.ts`, `src/plugins/api.ts`, `src/plugins/host.ts`, `src/plugins.ts`, `src/updater.ts` (only if needed — prefer subscribing from host.ts)

**Interfaces:**
- Produces: `EventMeta.params?: EventParam[]` with `export interface EventParam { name: string; type: string; doc?: string }`; `host.ui.Button/Input/Select`; bus events per spec; `HOST_VERSION = "1.7.0"`.

- [ ] **Step 1: bus.ts + test.** Add `params?: EventParam[]` to `EventMeta` (exported `EventParam`). Append one bus test: `describe()` with `params` round-trips through `known()`.

- [ ] **Step 2: api.ts.** Re-export `EventParam`; extend `TrawlUi`:

```ts
  Button: React.ComponentType<
    React.ButtonHTMLAttributes<HTMLButtonElement> & { variant?: string; size?: string }
  >;
  Input: React.ComponentType<React.InputHTMLAttributes<HTMLInputElement>>;
  Select: React.ComponentType<React.SelectHTMLAttributes<HTMLSelectElement>>;
```

(match the actual prop types of `src/components/ui/{button,input,select}.tsx` — narrow but assignable).

- [ ] **Step 3: host.ts.**
  - `HOST_VERSION = "1.7.0"`; `ui: { ..., Button, Input, Select }`.
  - Bridges: `flow-paused`→`breakpoint:hit`, `flow-resumed`→`breakpoint:resolved`, `breakpoint-timeout`→`breakpoint:timeout`, `rule-applied`→`rule:applied`, `rule-error`→`rule:error` (all `listen(...)` → `bus.emit`).
  - `flow:error` derivation: in the existing `flow-added`/`flow-updated` listeners, if `(payload as { state?: string }).state === "error"` and the flow id is unseen → `bus.emit("flow:error", payload)`. Seen-set: `Set<number>` FIFO-capped at 1000 (delete oldest via a parallel array or clear when > 1000).
  - `plugin:installed`/`plugin:removed`: in `src/plugins.ts`, at the end of successful `install` and `remove` store actions, `bus.emit("plugin:installed", { id, name, version })` / `bus.emit("plugin:removed", { id })` (import `bus` — check for import cycles; `plugins.ts` already sits beside the bus, and `host.ts` imports both; if a cycle appears, emit from the store via a small callback registered by host.ts instead — note which path you took).
  - `update:available`: subscribe `useUpdater` in `installHost()` like the other stores; on `status` transition to `"available"` → `bus.emit("update:available", { version, notes })`.
  - Describe ALL events (existing seven + eight new) with `params`. Reuse one shared `FLOW_PARAMS: EventParam[]` (id/timestamp/method/url.host/url.path/state/error/appliedRules/response.status — with one-line docs) for the Flow-payload events (`flow:added`, `flow:updated`, `flow:error`, `breakpoint:hit`, `breakpoint:resolved`, `breakpoint:timeout`); rule events get ruleName/phase/flowId/method/host/path(/error) params; `notify:send`, `filter:changed`, `project:changed`, `capture:*`, `plugin:*`, `update:available` get their own short lists. Keep `payloadType` strings in sync (add the new events' type expressions).

- [ ] **Step 4: `pnpm test && pnpm build` clean → commit** `feat(plugins): host API 1.7.0 — event params docs, breakpoint/rule/flow/plugin/update events, ui controls`.

---

### Task 3: Core docs + version bump

**Files:** `docs/plugins.md`, `package.json`, `src-tauri/Cargo.toml`(+lock), `src-tauri/tauri.conf.json`

- [ ] Extend the host-emitted events table with the 8 new rows (payload column matches the spec); document `params` in the registry section (one sentence + the `EventParam` shape) and the new `ui.Button/Input/Select`. Bump app version 0.5.0 → **0.6.0** (3 files + `cargo check` to refresh the lock). Verify `pnpm build` + `cargo check`. Commit `docs: v2 events + params registry; bump 0.6.0`.

---

## Part B — plugin (`trawl-plugin-notifications`)

### Task 4: Examples data + event-docs panel + UI overhaul + hints

**Files:**
- Create: `src/examples.ts`, `src/examples.test.ts`, `src/EventDocs.tsx`
- Modify: `src/trawl.d.ts` (mirror 1.7.0 additions: `EventParam`, `EventMeta.params`, `ui.Button/Input/Select`), `src/NotificationsApp.tsx`, `trawl-plugin.json` + `package.json` (0.2.0 / apiVersion 1.7.0)

**Interfaces:**
- Produces: `EXAMPLES: Record<string, { label: string; template?: string; condition?: string }[]>` keyed by event type with a `"*"` fallback list; `insertPath(path: string): string` (path → `{{payload.<path [] → [0]>}}`); `<EventDocs info={EventInfo} onInsert={(p) => …} />`.

- [ ] **Step 1: `src/examples.ts` + test.** Per-event examples (each entry has `label` + `template` and/or `condition`):
  - `flow:added` / `flow:updated`: «5xx alert» (`template: "🔥 {{payload.method}} {{payload.url.host}}{{payload.url.path}} → {{payload.response.status}}"`, `condition: "payload.response && payload.response.status >= 500"`), «slow request» (condition on timings if present), «any capture» template.
  - `flow:error`: «failed request» template with `{{payload.error}}`.
  - `breakpoint:hit`: «paused flow» template (method/host/path + `{{payload.pausedPhase}}`).
  - `breakpoint:timeout`: «auto-continued» template.
  - `rule:applied`: «rule fired» template (`{{payload.ruleName}}` on `{{payload.method}} {{payload.path}}`).
  - `rule:error`: «rule failed» template with `{{payload.error}}`, condition `true`.
  - `update:available`: «new version» template with `{{payload.version}}`.
  - `plugin:installed`: template with `{{payload.name}}`.
  - `"*"` fallback: «raw payload» (`{{payload}}`), «status ≥ 400 condition».
  Test: every example's `{{…}}` expressions are syntactically valid JS (`new Function("payload", "return (…)")` doesn't throw at construction for each extracted expr), and every event key that has a `condition` compiles.

- [ ] **Step 2: `src/EventDocs.tsx`.** Renders for the selected `EventInfo`: description line; params table (name — mono, clickable → `onInsert(name)`; type; doc; live example from `lastPayload` resolved by path when present, truncated ~40 chars); falls back to `host.util.inferFields([lastPayload])` rows when `params` is absent; "no payload observed yet" empty state. Small, presentational, host Tailwind classes.

- [ ] **Step 3: `NotificationsApp.tsx` overhaul.**
  - Replace every raw `<button>/<input>/<select>` with `host.ui.Button/Input/Select` (uniform heights; keep current handlers/state).
  - Template: `host.ui.ScriptEditor` with `language="handlebars"` in a bordered `h-24` wrapper (replacing the textarea); keep `setPayloadType` wiring.
  - Above template and condition: an **Examples** dropdown (host Select or a small Button+menu) listing `EXAMPLES[event] ?? []` merged with `EXAMPLES["*"]`; picking one replaces the corresponding field's content.
  - Mount `<EventDocs info={info} onInsert={(p) => setS({...s, template: s.template + insertPath(p)})} />` under the event selector (replaces the old chips row — EventDocs takes over the click-to-insert role).
  - Hints: helper lines under each form block — channels («Token from @BotFather, stored in Setup → Secrets», «chat_id: message @userinfobot or use a group id»), template («{{…}} runs JS with `payload` in scope»), condition («falsy → skip; errors skip too»), throttle («min seconds between sends»). Empty states point to the next action.
  - Event dropdown options show `type — description` (truncate description ~60 chars).

- [ ] **Step 4: versions** — plugin 0.2.0, apiVersion 1.7.0. `pnpm test && pnpm build` clean. Commit `feat: event docs panel, examples, native host controls, hints (0.2.0)`.

---

### Task 5: Plugin README touch + rebuild + push

- [ ] README: mention the doc panel, examples, and the new events (one short paragraph + bullet list of notable events); note host API ≥ 1.7.0. `pnpm build`; commit `docs: v2 events + examples; require host 1.7.0`; **push to origin main** (repo already published — keeping the served bundle current is part of publishing).

---

## Part C — finish

### Task 6: Verify + merge core

- [ ] Full sweep in the worktree: `(cd src-tauri && cargo test) && pnpm test && pnpm build`. Merge worktree branch → `main` (fast-forward expected), verify suites on main, remove worktree (`ExitWorktree remove` after confirming main == branch head). Do NOT push `http-catch` main (pending the user's earlier decision).
