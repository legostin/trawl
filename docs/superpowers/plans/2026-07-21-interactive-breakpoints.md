# Interactive Breakpoints Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a user hold a matching request or response in-flight, edit it live in the UI, then continue — armed either from a UI breakpoint list or from a rule via `ctx.breakpoint()`.

**Architecture:** The async proxy handler pauses a flow by registering a `oneshot` sender in a shared registry keyed by `(flow id, phase)`, emitting a `flow-paused` event, and `.await`ing the receiver. The UI resolves via a Tauri command that sends a `Resolution` (Execute / Abort / Respond) into the channel; the handler applies it and continues. Breakpoint definitions persist as JSON like rules do; `ctx.breakpoint()` sets a new script action the handler treats identically to a matched definition.

**Tech Stack:** Rust (Tauri 2, hudsucker proxy, rquickjs, tokio, serde), React 19 + TypeScript + zustand + Monaco.

## Global Constraints

- Paused flows are **never** persisted to SQLite; `persist()` runs only on resolve (completed/errored), as today.
- Timeout: hold indefinitely. If the registry sender is dropped (proxy stop / shutdown), the `.await` resolves to `Err` and the handler continues **unmodified** — never hang our side.
- Reuse existing matching: `crate::rules::glob_to_regex` for patterns, `active_scope()` for project scoping.
- Binary (non-text) bodies pass through unedited; only text bodies are editable.
- Rust JSON is camelCase via `#[serde(rename_all = "camelCase")]`. Frontend `Header = [string, string]`.
- Run checks from repo root: `pnpm exec tsc --noEmit`, `pnpm exec vitest run`; Rust: `cd src-tauri && cargo test`.

---

## File Structure

**Rust (`src-tauri/src/`):**
- `breakpoints.rs` (new) — `Breakpoint` type, load/save, matching.
- `proxy.rs` (modify) — `BpPhase`, `Resolution`, registry, pause/await in both phases, `ctx.breakpoint()` action handling.
- `scripting.rs` (modify) — `ctx.breakpoint()` injection, `"breakpoint"` action.
- `model.rs` (modify) — `paused_phase` field on `Flow`.
- `commands.rs` (modify) — breakpoint CRUD + `resolve_breakpoint`, `AppState` wiring.
- `lib.rs` (modify) — `mod breakpoints;`, register commands.

**Frontend (`src/`):**
- `breakpoints.ts` (new) — zustand store + `Breakpoint` type.
- `components/BreakpointsView.tsx` (new) — definitions management UI.
- `components/InterceptEditor.tsx` (new) — editable paused-flow editor.
- `types.ts` (modify) — `pausedPhase` on `Flow`.
- `store.ts` (modify) — `View` union, `flow-paused` listener, `resolveBreakpoint`, intercept toggle.
- `components/TopBar.tsx` (modify) — Breakpoints segment.
- `components/AppShell.tsx` (modify) — mount the Breakpoints pane.
- `components/FlowDetail.tsx` (modify) — render `InterceptEditor` when paused.
- `scripting/apiTypes.ts`, `scripting/stdlib.ts` (modify) — document `ctx.breakpoint()`.

---

## Task 1: Breakpoint type, persistence & matching (Rust)

**Files:**
- Create: `src-tauri/src/breakpoints.rs`
- Modify: `src-tauri/src/lib.rs:14` (add `mod breakpoints;`)

**Interfaces:**
- Produces:
  - `struct Breakpoint { id: String, name: String, enabled: bool, pattern: String, method: Option<String>, on_request: bool, on_response: bool, project_id: Option<String> }` (camelCase serde)
  - `impl Breakpoint { fn matches_target(&self, target: &str) -> bool }`
  - `fn load_breakpoints(dir: &Path) -> Result<Vec<Breakpoint>>`
  - `fn save_breakpoints(dir: &Path, bps: &[Breakpoint]) -> Result<()>`

- [ ] **Step 1: Write the failing test**

Create `src-tauri/src/breakpoints.rs` with only the test module and imports:

```rust
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests {
    use super::*;

    fn bp(pattern: &str) -> Breakpoint {
        Breakpoint {
            id: "1".into(),
            name: "t".into(),
            enabled: true,
            pattern: pattern.into(),
            method: None,
            on_request: true,
            on_response: false,
            project_id: None,
        }
    }

    #[test]
    fn matches_host_path_glob() {
        let b = bp("api.example.com/*");
        assert!(b.matches_target("api.example.com/v1/users"));
        assert!(!b.matches_target("cdn.example.com/v1/users"));
    }

    #[test]
    fn breakpoints_roundtrip_to_disk() {
        let tmp = std::env::temp_dir().join(format!("trawl-bp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(load_breakpoints(&tmp).unwrap().is_empty());
        save_breakpoints(&tmp, &[bp("api.example.com/*")]).unwrap();
        let back = load_breakpoints(&tmp).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].pattern, "api.example.com/*");
        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test breakpoints::`
Expected: FAIL — `cannot find type Breakpoint` / `cannot find function load_breakpoints`.

- [ ] **Step 3: Write minimal implementation**

Insert above the `#[cfg(test)]` module:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Breakpoint {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    /// glob over `host+path`, e.g. `api.example.com/*`, `*/login`.
    pub pattern: String,
    /// Optional method filter; None or "*" = any method.
    #[serde(default)]
    pub method: Option<String>,
    pub on_request: bool,
    pub on_response: bool,
    /// Owning project. None = global.
    #[serde(default)]
    pub project_id: Option<String>,
}

impl Breakpoint {
    pub fn matches_target(&self, target: &str) -> bool {
        match crate::rules::glob_to_regex(&self.pattern) {
            Ok(re) => re.is_match(target),
            Err(_) => false,
        }
    }
}

pub fn load_breakpoints(dir: &Path) -> Result<Vec<Breakpoint>> {
    let path = dir.join("breakpoints.json");
    if !path.exists() {
        return Ok(vec![]);
    }
    let text = fs::read_to_string(&path).context("read breakpoints.json")?;
    let bps = serde_json::from_str(&text).context("parse breakpoints.json")?;
    Ok(bps)
}

pub fn save_breakpoints(dir: &Path, bps: &[Breakpoint]) -> Result<()> {
    fs::create_dir_all(dir).context("create breakpoints dir")?;
    let text = serde_json::to_string_pretty(bps).context("serialize breakpoints")?;
    fs::write(dir.join("breakpoints.json"), text).context("write breakpoints.json")?;
    Ok(())
}
```

Then add `mod breakpoints;` to `src-tauri/src/lib.rs` after line 1 (`mod ca;`), keeping alphabetical-ish order near the top (place after `mod ca;`).

- [ ] **Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test breakpoints::`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/breakpoints.rs src-tauri/src/lib.rs
git commit -m "feat(breakpoints): Breakpoint type, persistence & matching"
```

---

## Task 2: `paused_phase` on the Flow model (Rust)

**Files:**
- Modify: `src-tauri/src/model.rs:50-83`

**Interfaces:**
- Produces: `Flow.paused_phase: Option<String>` (camelCase `pausedPhase`), defaulted; `Flow::new_request` sets it to `None`.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `src-tauri/src/model.rs`:

```rust
    #[test]
    fn flow_paused_phase_defaults_none_and_roundtrips() {
        let mut flow = Flow::new_request(
            1,
            "GET".into(),
            UrlParts { scheme: "http".into(), host: "h".into(), port: 80, path: "/".into() },
            HttpMessage { headers: vec![], body: vec![], body_is_text: true },
        );
        assert!(flow.paused_phase.is_none());
        flow.paused_phase = Some("request".into());
        let json = serde_json::to_string(&flow).unwrap();
        assert!(json.contains("\"pausedPhase\":\"request\""), "json was: {json}");
        let back: Flow = serde_json::from_str(&json).unwrap();
        assert_eq!(back.paused_phase.as_deref(), Some("request"));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test model::tests::flow_paused_phase`
Expected: FAIL — `no field paused_phase`.

- [ ] **Step 3: Write minimal implementation**

In the `Flow` struct (after `applied_rules`), add:

```rust
    /// Set while the flow is held on a breakpoint: "request" | "response".
    #[serde(default)]
    pub paused_phase: Option<String>,
```

In `Flow::new_request`, add `paused_phase: None,` to the returned struct literal (after `applied_rules: Vec::new(),`).

- [ ] **Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test model::`
Expected: PASS. (Other files that build `Flow` go through `new_request`, so no breakage.)

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/model.rs
git commit -m "feat(breakpoints): add paused_phase to Flow model"
```

---

## Task 3: `ctx.breakpoint()` script action (Rust)

**Files:**
- Modify: `src-tauri/src/scripting.rs:130-163` (`build_source`)

**Interfaces:**
- Produces: request/response rules may return `ScriptResult { action: "breakpoint", .. }` when the script calls `ctx.breakpoint()`. `ScriptResult` already has an untyped `action: String`, so no struct change is needed.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `src-tauri/src/scripting.rs`:

```rust
    #[tokio::test]
    async fn script_can_request_breakpoint() {
        let res = run("ctx.breakpoint();", r#"{"request":{"headers":{}}}"#).await;
        assert_eq!(res.action, "breakpoint");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test scripting::tests::script_can_request_breakpoint`
Expected: FAIL — action is `"continue"` (the `ctx.breakpoint` call throws `TypeError: not a function`, becoming an error result), assertion fails.

- [ ] **Step 3: Write minimal implementation**

In `build_source`, alongside the `ctx.mock` / `ctx.abort` definitions (inside the generated IIFE), add:

```javascript
    ctx.breakpoint = function() { ctx.__action = "breakpoint"; };
```

Place it immediately after the `ctx.abort = ...` line.

- [ ] **Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test scripting::`
Expected: PASS (all scripting tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/scripting.rs
git commit -m "feat(breakpoints): ctx.breakpoint() script action"
```

---

## Task 4: Proxy pause infrastructure + request-phase breakpoint (Rust)

**Files:**
- Modify: `src-tauri/src/proxy.rs` (imports, `CaptureHandler` fields, `Directive` enum, new types & helpers, `handle_request`, `start` signature, test helpers)

**Interfaces:**
- Produces (used by Task 5 and Task 6):
  - `#[derive(Clone, Copy, PartialEq, Eq, Hash)] pub enum BpPhase { Request, Response }`
  - `pub enum Resolution { Execute { method: Option<String>, status: Option<u16>, headers: Vec<(String, String)>, body: String }, Abort(String), Respond { status: u16, headers: Vec<(String, String)>, body: String } }`
  - `pub type BreakpointRegistry = Arc<Mutex<HashMap<(u64, BpPhase), oneshot::Sender<Resolution>>>>;`
  - `pub type SharedBreakpoints = Arc<RwLock<Vec<crate::breakpoints::Breakpoint>>>;`
  - `pub type SharedIntercept = Arc<RwLock<bool>>;`
  - `start(...)` gains three trailing params: `breakpoints: SharedBreakpoints, intercept: SharedIntercept, pending: BreakpointRegistry`.
- Consumes: `crate::breakpoints::Breakpoint` (Task 1), `Flow.paused_phase` (Task 2), `"breakpoint"` action (Task 3).

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `src-tauri/src/proxy.rs`. First extend the `scripting` test helper to also return breakpoint state, then add the test. Replace the existing `scripting` helper:

```rust
    fn scripting(
        rules: Vec<Rule>,
    ) -> (
        ScriptClient,
        SharedRules,
        SharedLibrary,
        SharedProject,
        SharedBreakpoints,
        SharedIntercept,
        BreakpointRegistry,
    ) {
        (
            spawn_engine(Duration::from_millis(500)),
            Arc::new(RwLock::new(rules)),
            Arc::new(RwLock::new(String::new())),
            Arc::new(RwLock::new(None)),
            Arc::new(RwLock::new(vec![])),
            Arc::new(RwLock::new(true)),
            Arc::new(Mutex::new(std::collections::HashMap::new())),
        )
    }
```

Every existing call site of `scripting(...)` and `start(...)` in this test module must be updated (see Step 3). Add the new test:

```rust
    fn breakpoint(pattern: &str, on_request: bool, on_response: bool) -> crate::breakpoints::Breakpoint {
        crate::breakpoints::Breakpoint {
            id: "b".into(),
            name: "b".into(),
            enabled: true,
            pattern: pattern.into(),
            method: None,
            on_request,
            on_response,
            project_id: None,
        }
    }

    // A request breakpoint holds the flow; resolving Execute with an edited header
    // sends the edit to the upstream echo server.
    #[tokio::test]
    async fn request_breakpoint_execute_applies_edit() {
        let upstream_addr = echo_upstream().await;
        let store = FlowStore::new(10);
        let emit: EmitFn = Arc::new(|_e, _f| {});
        let (s, r, l, p, bps, icept, pending) = scripting(vec![]);
        *bps.write().unwrap() = vec![breakpoint(&format!("{upstream_addr}/*"), true, false)];
        let ca_dir = temp_ca();
        let handle = start(
            "127.0.0.1:0".parse().unwrap(), store.clone(), emit, ca_dir.clone(),
            s, r, l, p, ca_dir.clone(), None, bps, icept, pending.clone(),
        ).await.unwrap();
        let bound = handle.local_addr();

        // Resolver task: wait until the flow is paused, then Execute with an edit.
        let pending2 = pending.clone();
        let store2 = store.clone();
        tokio::spawn(async move {
            loop {
                let paused = store2.all().into_iter().find(|f| f.state == FlowState::Paused);
                if let Some(f) = paused {
                    if let Some(tx) = pending2.lock().unwrap().remove(&(f.id, BpPhase::Request)) {
                        let _ = tx.send(Resolution::Execute {
                            method: None,
                            status: None,
                            headers: vec![("X-Debug".into(), "edited".into())],
                            body: String::new(),
                        });
                        break;
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });

        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(format!("http://{bound}")).unwrap())
            .build().unwrap();
        let echoed = client.get(format!("http://{upstream_addr}/api"))
            .send().await.unwrap().text().await.unwrap();
        assert!(echoed.to_lowercase().contains("x-debug: edited"), "edit not applied: {echoed}");

        handle.stop();
        let _ = std::fs::remove_dir_all(&ca_dir);
    }

    // Abort resolution short-circuits with 502.
    #[tokio::test]
    async fn request_breakpoint_abort_returns_502() {
        let upstream_addr = echo_upstream().await;
        let store = FlowStore::new(10);
        let emit: EmitFn = Arc::new(|_e, _f| {});
        let (s, r, l, p, bps, icept, pending) = scripting(vec![]);
        *bps.write().unwrap() = vec![breakpoint(&format!("{upstream_addr}/*"), true, false)];
        let ca_dir = temp_ca();
        let handle = start(
            "127.0.0.1:0".parse().unwrap(), store.clone(), emit, ca_dir.clone(),
            s, r, l, p, ca_dir.clone(), None, bps, icept, pending.clone(),
        ).await.unwrap();
        let bound = handle.local_addr();

        let pending2 = pending.clone();
        let store2 = store.clone();
        tokio::spawn(async move {
            loop {
                if let Some(f) = store2.all().into_iter().find(|f| f.state == FlowState::Paused) {
                    if let Some(tx) = pending2.lock().unwrap().remove(&(f.id, BpPhase::Request)) {
                        let _ = tx.send(Resolution::Abort("nope".into()));
                        break;
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });

        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(format!("http://{bound}")).unwrap())
            .build().unwrap();
        let status = client.get(format!("http://{upstream_addr}/api"))
            .send().await.unwrap().status();
        assert_eq!(status, 502);

        handle.stop();
        let _ = std::fs::remove_dir_all(&ca_dir);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test proxy::tests::request_breakpoint`
Expected: FAIL to compile — `BpPhase`, `Resolution`, `SharedBreakpoints`, `SharedIntercept`, `BreakpointRegistry` undefined; `start` arity mismatch.

- [ ] **Step 3: Write minimal implementation**

**(a) Imports.** At the top of `proxy.rs`, extend the `std` imports:

```rust
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
```

(Add `HashMap` and `Mutex`.)

**(b) Types.** After the existing `pub type SharedProject = ...;` line, add:

```rust
pub type SharedBreakpoints = Arc<RwLock<Vec<crate::breakpoints::Breakpoint>>>;
pub type SharedIntercept = Arc<RwLock<bool>>;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum BpPhase {
    Request,
    Response,
}

pub enum Resolution {
    Execute {
        method: Option<String>,
        status: Option<u16>,
        headers: Vec<(String, String)>,
        body: String,
    },
    Abort(String),
    Respond {
        status: u16,
        headers: Vec<(String, String)>,
        body: String,
    },
}

pub type BreakpointRegistry = Arc<Mutex<HashMap<(u64, BpPhase), oneshot::Sender<Resolution>>>>;
```

**(c) Handler fields.** Add three fields to `struct CaptureHandler`:

```rust
    breakpoints: SharedBreakpoints,
    intercept: SharedIntercept,
    pending: BreakpointRegistry,
```

**(d) Directive.** Extend the enum:

```rust
enum Directive {
    Continue,
    Mock(Value),
    Abort(String),
    Breakpoint,
}
```

**(e) Helpers.** Add to the `impl CaptureHandler` block (near `matching`):

```rust
    /// Does any enabled, in-scope breakpoint match this flow in `phase`?
    fn breakpoint_matches(&self, phase: BpPhase, targets: &[String], method: &str) -> bool {
        if !*self.intercept.read().unwrap() {
            return false;
        }
        let scope = self.active_scope();
        self.breakpoints.read().unwrap().iter().any(|b| {
            b.enabled
                && b.project_id == scope
                && match phase {
                    BpPhase::Request => b.on_request,
                    BpPhase::Response => b.on_response,
                }
                && b.method
                    .as_deref()
                    .map_or(true, |m| m == "*" || m.eq_ignore_ascii_case(method))
                && targets.iter().any(|t| b.matches_target(t))
        })
    }

    /// Register a pending breakpoint and await the UI's resolution.
    /// Returns None if the sender was dropped (proxy stopped) — caller continues unmodified.
    async fn await_resolution(&self, id: u64, phase: BpPhase) -> Option<Resolution> {
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert((id, phase), tx);
        rx.await.ok()
    }
```

**(f) `handle_request` — pause hook.** The current tail of `handle_request` builds the flow, inserts it, then `match`es `directive`. Replace the block from `let out_body: Vec<u8> = ...` through the final `match directive { ... }` with this (it inserts the flow first, then pauses if needed, then computes bytes and matches):

```rust
        let mut override_method: Option<String> = None;
        let mut flow = Flow::new_request(
            id,
            parts.method.to_string(),
            url,
            HttpMessage { headers: work_headers.clone(), body: if is_text { work_body.clone().into_bytes() } else { display_body.clone() }, body_is_text: is_text },
        );
        flow.timestamp = unix_ms();
        flow.timings.sent = Some(self.started.elapsed().as_millis() as u64);
        flow.applied_rules = applied;
        flow.error = script_error;
        self.store.insert(flow.clone());
        (self.emit)("flow-added", &flow);
        self.current_id = Some(id);

        // Pause on a matched UI breakpoint or a script ctx.breakpoint().
        let want_break = matches!(directive, Directive::Breakpoint)
            || self.breakpoint_matches(BpPhase::Request, &targets, &parts.method.to_string());
        if want_break {
            self.store.update(id, |f| {
                f.state = FlowState::Paused;
                f.paused_phase = Some("request".into());
            });
            if let Some(u) = self.store.all().into_iter().find(|f| f.id == id) {
                (self.emit)("flow-paused", &u);
            }
            match self.await_resolution(id, BpPhase::Request).await {
                Some(Resolution::Execute { method, headers, body, .. }) => {
                    if let Some(m) = method {
                        override_method = Some(m);
                    }
                    work_headers = headers;
                    if is_text {
                        work_body = body;
                    }
                    directive = Directive::Continue;
                }
                Some(Resolution::Abort(reason)) => directive = Directive::Abort(reason),
                Some(Resolution::Respond { status, headers, body }) => {
                    directive = Directive::Mock(json!({ "status": status, "headers": headers_to_json(&headers), "body": body }));
                }
                None => directive = Directive::Continue,
            }
            self.store.update(id, |f| {
                f.state = FlowState::Pending;
                f.paused_phase = None;
                f.request.headers = work_headers.clone();
                if is_text {
                    f.request.body = work_body.clone().into_bytes();
                }
                if let Some(m) = &override_method {
                    f.method = m.clone();
                }
            });
            if let Some(u) = self.store.all().into_iter().find(|f| f.id == id) {
                (self.emit)("flow-updated", &u);
            }
        }

        let out_body: Vec<u8> = if is_text { work_body.clone().into_bytes() } else { bytes.clone() };

        match directive {
            Directive::Mock(spec) => {
                self.record_mock_response(id, &spec);
                RequestOrResponse::Response(build_mock_response(&spec))
            }
            Directive::Abort(reason) => {
                self.store.update(id, |f| {
                    f.state = FlowState::Error;
                    f.error = Some(reason.clone());
                });
                if let Some(u) = self.store.all().into_iter().find(|f| f.id == id) {
                    (self.emit)("flow-updated", &u);
                    self.persist(&u);
                }
                RequestOrResponse::Response(build_abort_response(&reason))
            }
            Directive::Breakpoint | Directive::Continue => {
                let mut new_parts = parts;
                if let Some(m) = override_method {
                    if let Ok(method) = hudsucker::hyper::Method::from_bytes(m.as_bytes()) {
                        new_parts.method = method;
                    }
                }
                new_parts.headers = build_header_map(&work_headers, out_body.len(), is_text);
                Request::from_parts(new_parts, Body::from(Full::new(Bytes::from(out_body)))).into()
            }
        }
```

Notes:
- `work_headers`, `work_body`, `directive`, `applied`, `script_error` are already declared `mut` earlier in the function; the previous non-mut `flow`/`stored_body` locals are replaced by the block above. Remove the now-duplicated old `let mut flow = ...; ... self.persist(&flow);` and old `match directive` that followed — this block supersedes them.
- The `persist(&flow)` on the normal add path is intentionally dropped here for paused flows; keep the request flow **out** of the DB until the response completes (matches existing behavior — the request-only insert already didn't guarantee persistence semantics; the response phase persists the final flow). If the original code called `self.persist(&flow)` right after `flow-added`, keep that call for the non-paused path by adding `if !want_break { self.persist(&flow); }` immediately after the `(self.emit)("flow-added", &flow);` line.

**(g) In the `matching_handler` early-return handler-mode block**, no change needed (handler rules never break).

**(h) `start` signature.** Add three params at the end and pass them into the handler literal:

```rust
pub async fn start(
    addr: SocketAddr,
    store: FlowStore,
    emit: EmitFn,
    ca_dir: PathBuf,
    scripts: ScriptClient,
    rules: SharedRules,
    library: SharedLibrary,
    active_project: SharedProject,
    data_dir: PathBuf,
    db: Option<DbHandle>,
    breakpoints: SharedBreakpoints,
    intercept: SharedIntercept,
    pending: BreakpointRegistry,
) -> Result<ProxyHandle> {
```

In the `CaptureHandler { ... }` literal add `breakpoints, intercept, pending,`.

**(i) Fix every existing test call site.** In each existing test that calls `let (s, r, l, p) = scripting(...)`, change to `let (s, r, l, p, bps, icept, pending) = scripting(...)` and append `, bps, icept, pending` to the corresponding `start(...)` call (before `.await`). There are multiple (`captures_http_flow_through_proxy`, `serves_ca_pem_on_magic_host`, `decompresses_gzip_response_body`, `decrypts_https_through_proxy`, `request_rule_adds_header_reaches_upstream`, `request_rule_mock_short_circuits`, `handler_rule_sends_and_transforms_response`, `handler_send_preserves_path_query_headers`, `untracked_host_not_stored`, `tracked_host_uses_only_project_rules`, `gzip_response_reaches_client_decodable`).

- [ ] **Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test proxy::`
Expected: PASS — including `request_breakpoint_execute_applies_edit` and `request_breakpoint_abort_returns_502`, and all pre-existing proxy tests still green.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/proxy.rs
git commit -m "feat(breakpoints): pause & resolve request-phase flows in proxy"
```

---

## Task 5: Response-phase breakpoint (Rust)

**Files:**
- Modify: `src-tauri/src/proxy.rs` `handle_response` (after the response-rules loop, before building `new_parts`)

**Interfaces:**
- Consumes: `BpPhase::Response`, `Resolution`, `breakpoint_matches`, `await_resolution` (Task 4).

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `src-tauri/src/proxy.rs`:

```rust
    // A response breakpoint edits status + body before the client receives it.
    #[tokio::test]
    async fn response_breakpoint_execute_edits_response() {
        let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = upstream.accept().await.unwrap();
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut b = [0u8; 1024];
                    let _ = sock.read(&mut b).await;
                    let _ = sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 4\r\n\r\norig").await;
                });
            }
        });

        let store = FlowStore::new(10);
        let emit: EmitFn = Arc::new(|_e, _f| {});
        let (s, r, l, p, bps, icept, pending) = scripting(vec![]);
        *bps.write().unwrap() = vec![breakpoint(&format!("{upstream_addr}/*"), false, true)];
        let ca_dir = temp_ca();
        let handle = start(
            "127.0.0.1:0".parse().unwrap(), store.clone(), emit, ca_dir.clone(),
            s, r, l, p, ca_dir.clone(), None, bps, icept, pending.clone(),
        ).await.unwrap();
        let bound = handle.local_addr();

        let pending2 = pending.clone();
        let store2 = store.clone();
        tokio::spawn(async move {
            loop {
                if let Some(f) = store2.all().into_iter().find(|f| f.paused_phase.as_deref() == Some("response")) {
                    if let Some(tx) = pending2.lock().unwrap().remove(&(f.id, BpPhase::Response)) {
                        let _ = tx.send(Resolution::Execute {
                            method: None,
                            status: Some(418),
                            headers: vec![("Content-Type".into(), "text/plain".into())],
                            body: "edited".into(),
                        });
                        break;
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });

        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(format!("http://{bound}")).unwrap())
            .build().unwrap();
        let resp = client.get(format!("http://{upstream_addr}/api")).send().await.unwrap();
        assert_eq!(resp.status(), 418);
        assert_eq!(resp.text().await.unwrap(), "edited");

        handle.stop();
        let _ = std::fs::remove_dir_all(&ca_dir);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test proxy::tests::response_breakpoint_execute_edits_response`
Expected: FAIL — response is `200`/`orig` (no pause implemented in `handle_response`).

- [ ] **Step 3: Write minimal implementation**

In `handle_response`, after the response-rules loop (`self.apply_env(&env);` closing the `if !rules.is_empty()` block) and **before** the `let out_body ...` line, insert:

```rust
        // Pause on a matched response breakpoint (UI-defined). Response-phase
        // rules can also request it via ctx.breakpoint() -> action "breakpoint".
        let want_break = self.breakpoint_matches(BpPhase::Response, &targets, "");
        if want_break {
            self.store.update(id, |f| {
                f.state = FlowState::Paused;
                f.paused_phase = Some("response".into());
                f.response = Some(ResponseMessage {
                    status: work_status,
                    headers: work_headers.clone(),
                    body: if is_text { work_body.clone().into_bytes() } else { display_body.clone() },
                    body_is_text: is_text,
                });
            });
            if let Some(u) = self.store.all().into_iter().find(|f| f.id == id) {
                (self.emit)("flow-paused", &u);
            }
            match self.await_resolution(id, BpPhase::Response).await {
                Some(Resolution::Execute { status, headers, body, .. }) => {
                    if let Some(sc) = status {
                        work_status = sc;
                    }
                    work_headers = headers;
                    if is_text {
                        work_body = body;
                    }
                }
                Some(Resolution::Abort(reason)) => {
                    self.store.update(id, |f| {
                        f.state = FlowState::Error;
                        f.paused_phase = None;
                        f.error = Some(reason.clone());
                    });
                    if let Some(u) = self.store.all().into_iter().find(|f| f.id == id) {
                        (self.emit)("flow-updated", &u);
                        self.persist(&u);
                    }
                    return build_abort_response(&reason);
                }
                // Respond has no meaning in the response phase; treat as continue.
                Some(Resolution::Respond { .. }) | None => {}
            }
            self.store.update(id, |f| {
                f.state = FlowState::Pending;
                f.paused_phase = None;
            });
        }
```

Also add the `"breakpoint"` action handling inside the response-rules loop `match res.action.as_str()` so a response rule's `ctx.breakpoint()` sets a flag. Simplest: extend the `want_break` computation to also honor a rule-triggered break. Add a `let mut rule_break = false;` next to `let mut script_error` above the loop, add a match arm:

```rust
                    "breakpoint" => {
                        if let Some(rv) = &res.response {
                            if let Some(s) = rv.get("status").and_then(|s| s.as_u64()) { work_status = s as u16; }
                            if let Some(h) = rv.get("headers") { work_headers = json_to_headers(h); }
                            if let Some(b) = rv.get("body").and_then(|b| b.as_str()) { work_body = b.to_string(); }
                        }
                        rule_break = true;
                        applied.push(rule.name.clone());
                    }
```

and change the `want_break` line to:

```rust
        let want_break = rule_break || self.breakpoint_matches(BpPhase::Response, &targets, "");
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test proxy::`
Expected: PASS (all proxy tests including the new response one).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/proxy.rs
git commit -m "feat(breakpoints): pause & resolve response-phase flows in proxy"
```

---

## Task 6: Tauri commands & AppState wiring (Rust)

**Files:**
- Modify: `src-tauri/src/commands.rs` (`AppState`, `start_proxy`, new commands)
- Modify: `src-tauri/src/lib.rs:33-75` (register commands)

**Interfaces:**
- Produces (invoked from frontend): `list_breakpoints`, `save_breakpoint`, `delete_breakpoint`, `set_intercept`, `get_intercept`, `resolve_breakpoint`.
- Consumes: `proxy::{SharedBreakpoints, SharedIntercept, BreakpointRegistry, BpPhase, Resolution}` (Task 4), `breakpoints::{Breakpoint, load_breakpoints, save_breakpoints}` (Task 1).

- [ ] **Step 1: Write the failing test**

Rust command bodies are thin wrappers exercised via the proxy integration tests already written; add one focused unit test for the resolve routing. Add a `#[cfg(test)] mod tests` at the bottom of `commands.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy::{BpPhase, BreakpointRegistry, Resolution};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn resolve_sends_into_registry() {
        let pending: BreakpointRegistry = Arc::new(Mutex::new(HashMap::new()));
        let (tx, rx) = oneshot::channel::<Resolution>();
        pending.lock().unwrap().insert((7, BpPhase::Request), tx);

        // Simulate what resolve_breakpoint's core does:
        let taken = pending.lock().unwrap().remove(&(7, BpPhase::Request));
        assert!(taken.is_some());
        let _ = taken.unwrap().send(Resolution::Abort("x".into()));

        match rx.await.unwrap() {
            Resolution::Abort(r) => assert_eq!(r, "x"),
            _ => panic!("wrong resolution"),
        }
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test commands::tests::resolve_sends_into_registry`
Expected: FAIL to compile until `AppState` and imports exist (the test only needs `proxy` types, so if Task 4 is done it may compile; if so this test PASSES trivially and simply guards the registry contract — acceptable). If it passes immediately, proceed; the real coverage is the proxy integration tests.

- [ ] **Step 3: Write minimal implementation**

**(a) `AppState`.** Add fields:

```rust
    pub breakpoints: Arc<RwLock<Vec<crate::breakpoints::Breakpoint>>>,
    pub intercept: Arc<RwLock<bool>>,
    pub pending_breakpoints: crate::proxy::BreakpointRegistry,
```

In `AppState::new()` initialize:

```rust
            breakpoints: Arc::new(RwLock::new(Vec::new())),
            intercept: Arc::new(RwLock::new(true)),
            pending_breakpoints: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
```

**(b) `start_proxy`.** Before the `proxy::start(...)` call, load breakpoints into the shared cell:

```rust
    let loaded_bps = crate::breakpoints::load_breakpoints(&rdir).map_err(|e| e.to_string())?;
    *state.breakpoints.write().unwrap() = loaded_bps;
```

Then append the three new args to `proxy::start(...)`:

```rust
        state.breakpoints.clone(),
        state.intercept.clone(),
        state.pending_breakpoints.clone(),
```

(as the last three positional args, after `state.db.get().cloned(),`).

**(c) New commands** (add near the rules commands):

```rust
#[tauri::command]
pub fn list_breakpoints(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<crate::breakpoints::Breakpoint>, String> {
    let loaded = crate::breakpoints::load_breakpoints(&rules_dir(&app)?).map_err(|e| e.to_string())?;
    *state.breakpoints.write().unwrap() = loaded.clone();
    Ok(loaded)
}

#[tauri::command]
pub fn save_breakpoint(app: AppHandle, breakpoint: crate::breakpoints::Breakpoint, state: State<'_, AppState>) -> Result<Vec<crate::breakpoints::Breakpoint>, String> {
    let dir = rules_dir(&app)?;
    let mut bps = crate::breakpoints::load_breakpoints(&dir).map_err(|e| e.to_string())?;
    if let Some(existing) = bps.iter_mut().find(|b| b.id == breakpoint.id) {
        *existing = breakpoint;
    } else {
        bps.push(breakpoint);
    }
    crate::breakpoints::save_breakpoints(&dir, &bps).map_err(|e| e.to_string())?;
    *state.breakpoints.write().unwrap() = bps.clone();
    Ok(bps)
}

#[tauri::command]
pub fn delete_breakpoint(app: AppHandle, id: String, state: State<'_, AppState>) -> Result<Vec<crate::breakpoints::Breakpoint>, String> {
    let dir = rules_dir(&app)?;
    let mut bps = crate::breakpoints::load_breakpoints(&dir).map_err(|e| e.to_string())?;
    bps.retain(|b| b.id != id);
    crate::breakpoints::save_breakpoints(&dir, &bps).map_err(|e| e.to_string())?;
    *state.breakpoints.write().unwrap() = bps.clone();
    Ok(bps)
}

#[tauri::command]
pub fn set_intercept(enabled: bool, state: State<'_, AppState>) {
    *state.intercept.write().unwrap() = enabled;
}

#[tauri::command]
pub fn get_intercept(state: State<'_, AppState>) -> bool {
    *state.intercept.read().unwrap()
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditedPayload {
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub status: Option<u16>,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub reason: Option<String>,
}

#[tauri::command]
pub fn resolve_breakpoint(
    id: u64,
    phase: String,
    action: String,
    edited: EditedPayload,
    state: State<'_, AppState>,
) -> Result<(), String> {
    use crate::proxy::{BpPhase, Resolution};
    let bp_phase = match phase.as_str() {
        "request" => BpPhase::Request,
        "response" => BpPhase::Response,
        _ => return Err("bad phase".into()),
    };
    let resolution = match action.as_str() {
        "execute" => Resolution::Execute {
            method: edited.method,
            status: edited.status,
            headers: edited.headers,
            body: edited.body,
        },
        "abort" => Resolution::Abort(edited.reason.unwrap_or_else(|| "aborted".into())),
        "respond" => Resolution::Respond {
            status: edited.status.unwrap_or(200),
            headers: edited.headers,
            body: edited.body,
        },
        _ => return Err("bad action".into()),
    };
    let tx = state.pending_breakpoints.lock().unwrap().remove(&(id, bp_phase));
    match tx {
        Some(tx) => {
            let _ = tx.send(resolution);
            Ok(())
        }
        None => Err("no pending breakpoint".into()),
    }
}
```

**(d) Register in `lib.rs`** — add to the `tauri::generate_handler![...]` list (after `commands::delete_rule,`):

```rust
            commands::list_breakpoints,
            commands::save_breakpoint,
            commands::delete_breakpoint,
            commands::set_intercept,
            commands::get_intercept,
            commands::resolve_breakpoint,
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test`
Expected: PASS (whole Rust suite). Also run `cargo build` to confirm the Tauri command macros compile.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat(breakpoints): Tauri commands (CRUD, intercept toggle, resolve)"
```

---

## Task 7: Frontend Breakpoint type & store (TS)

**Files:**
- Create: `src/breakpoints.ts`
- Test: `src/breakpoints.test.ts`

**Interfaces:**
- Produces:
  - `interface Breakpoint { id: string; name: string; enabled: boolean; pattern: string; method: string | null; onRequest: boolean; onResponse: boolean; projectId: string | null }`
  - `useBreakpoints` zustand store: `{ breakpoints: Breakpoint[]; intercept: boolean; load(): Promise<void>; upsert(bp): Promise<void>; remove(id): Promise<void>; setIntercept(on: boolean): Promise<void> }`

- [ ] **Step 1: Write the failing test**

Create `src/breakpoints.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

import { useBreakpoints } from "./breakpoints";

const bp = (id: string) => ({
  id, name: "n", enabled: true, pattern: "*/*",
  method: null, onRequest: true, onResponse: false, projectId: null,
});

describe("breakpoints store", () => {
  beforeEach(() => {
    invoke.mockReset();
    useBreakpoints.setState({ breakpoints: [], intercept: true });
  });

  it("load pulls definitions and intercept flag", async () => {
    invoke.mockImplementation((cmd: string) =>
      cmd === "list_breakpoints" ? Promise.resolve([bp("a")]) : Promise.resolve(true));
    await useBreakpoints.getState().load();
    expect(useBreakpoints.getState().breakpoints).toHaveLength(1);
    expect(useBreakpoints.getState().intercept).toBe(true);
  });

  it("upsert saves and stores the returned list", async () => {
    invoke.mockResolvedValue([bp("a"), bp("b")]);
    await useBreakpoints.getState().upsert(bp("b"));
    expect(invoke).toHaveBeenCalledWith("save_breakpoint", { breakpoint: bp("b") });
    expect(useBreakpoints.getState().breakpoints).toHaveLength(2);
  });

  it("setIntercept invokes and updates state", async () => {
    invoke.mockResolvedValue(undefined);
    await useBreakpoints.getState().setIntercept(false);
    expect(invoke).toHaveBeenCalledWith("set_intercept", { enabled: false });
    expect(useBreakpoints.getState().intercept).toBe(false);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm exec vitest run src/breakpoints.test.ts`
Expected: FAIL — cannot resolve `./breakpoints`.

- [ ] **Step 3: Write minimal implementation**

Create `src/breakpoints.ts`:

```ts
import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";

export interface Breakpoint {
  id: string;
  name: string;
  enabled: boolean;
  /** host/path glob, e.g. api.example.com/* */
  pattern: string;
  /** Method filter; null or "*" = any. */
  method: string | null;
  onRequest: boolean;
  onResponse: boolean;
  /** Owning project; null = global. */
  projectId: string | null;
}

interface BreakpointsState {
  breakpoints: Breakpoint[];
  intercept: boolean;
  selectedId: string | null;
  load: () => Promise<void>;
  select: (id: string | null) => void;
  upsert: (bp: Breakpoint) => Promise<void>;
  remove: (id: string) => Promise<void>;
  setIntercept: (enabled: boolean) => Promise<void>;
}

export const useBreakpoints = create<BreakpointsState>((set) => ({
  breakpoints: [],
  intercept: true,
  selectedId: null,
  load: async () => {
    const [breakpoints, intercept] = await Promise.all([
      invoke<Breakpoint[]>("list_breakpoints"),
      invoke<boolean>("get_intercept"),
    ]);
    set({ breakpoints, intercept });
  },
  select: (id) => set({ selectedId: id }),
  upsert: async (bp) => {
    const breakpoints = await invoke<Breakpoint[]>("save_breakpoint", { breakpoint: bp });
    set({ breakpoints, selectedId: bp.id });
  },
  remove: async (id) => {
    const breakpoints = await invoke<Breakpoint[]>("delete_breakpoint", { id });
    set({ breakpoints, selectedId: null });
  },
  setIntercept: async (enabled) => {
    await invoke("set_intercept", { enabled });
    set({ intercept: enabled });
  },
}));
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm exec vitest run src/breakpoints.test.ts`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add src/breakpoints.ts src/breakpoints.test.ts
git commit -m "feat(breakpoints): frontend Breakpoint store"
```

---

## Task 8: Flow paused state in the flows store (TS)

**Files:**
- Modify: `src/types.ts:32-43` (add `pausedPhase`)
- Modify: `src/store.ts` (`View` union, `flow-paused` listener, `resolveBreakpoint`)
- Test: `src/store.test.ts` (create)

**Interfaces:**
- Produces:
  - `Flow.pausedPhase?: "request" | "response" | null`
  - `View = "traffic" | "rules" | "breakpoints"`
  - `resolveBreakpoint(id: number, phase: "request" | "response", action: "execute" | "abort" | "respond", edited: EditedPayload): Promise<void>` where `EditedPayload = { method?: string; status?: number; headers?: Header[]; body?: string; reason?: string }`
  - `pausedCount` derived via selector in components (no store field needed).

- [ ] **Step 1: Write the failing test**

Create `src/store.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

import { useFlows } from "./store";
import type { Flow } from "./types";

const flow = (id: number, patch: Partial<Flow> = {}): Flow => ({
  id, timestamp: 0, method: "GET",
  url: { scheme: "http", host: "h", port: 80, path: "/" },
  request: { headers: [], body: "", bodyIsText: true },
  response: null,
  timings: { sent: null, ttfb: null, done: null },
  state: "pending", error: null, appliedRules: [], ...patch,
});

describe("flows store — breakpoints", () => {
  beforeEach(() => {
    invoke.mockReset();
    useFlows.setState({ flows: [], selectedId: null });
  });

  it("upsert reflects a paused flow", () => {
    useFlows.getState().upsert(flow(1, { state: "paused", pausedPhase: "request" }));
    const f = useFlows.getState().flows[0];
    expect(f.state).toBe("paused");
    expect(f.pausedPhase).toBe("request");
  });

  it("resolveBreakpoint invokes resolve_breakpoint with the payload", async () => {
    invoke.mockResolvedValue(undefined);
    await useFlows.getState().resolveBreakpoint(1, "request", "abort", { reason: "no" });
    expect(invoke).toHaveBeenCalledWith("resolve_breakpoint", {
      id: 1, phase: "request", action: "abort", edited: { reason: "no" },
    });
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm exec vitest run src/store.test.ts`
Expected: FAIL — `resolveBreakpoint` is not a function / `pausedPhase` type error.

- [ ] **Step 3: Write minimal implementation**

In `src/types.ts`, add to `Flow`:

```ts
  /** Set while the flow is held on a breakpoint. */
  pausedPhase?: "request" | "response" | null;
```

In `src/store.ts`:

- Change the `View` type: `export type View = "traffic" | "rules" | "breakpoints";`
- Add an `EditedPayload` type and the `resolveBreakpoint` signature to the `FlowsState` interface:

```ts
export interface EditedPayload {
  method?: string;
  status?: number;
  headers?: [string, string][];
  body?: string;
  reason?: string;
}
```

Add to the interface: `resolveBreakpoint: (id: number, phase: "request" | "response", action: "execute" | "abort" | "respond", edited: EditedPayload) => Promise<void>;`

- In the store body, add the implementation:

```ts
  resolveBreakpoint: async (id, phase, action, edited) => {
    await invoke("resolve_breakpoint", { id, phase, action, edited });
  },
```

- In `init`, add a listener for `flow-paused` next to the existing two:

```ts
    const un3 = await listen<Flow>("flow-paused", (e) => {
      get().upsert(e.payload);
      set({ selectedId: e.payload.id });
    });
```

and return a cleanup that also calls `un3()`.

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm exec vitest run src/store.test.ts && pnpm exec tsc --noEmit`
Expected: PASS (2 tests) and no type errors.

- [ ] **Step 5: Commit**

```bash
git add src/types.ts src/store.ts src/store.test.ts
git commit -m "feat(breakpoints): paused flow state & resolveBreakpoint in flows store"
```

---

## Task 9: Breakpoints management view + TopBar segment + AppShell pane (TS)

**Files:**
- Create: `src/components/BreakpointsView.tsx`
- Modify: `src/components/TopBar.tsx:86-93` (Segmented options)
- Modify: `src/components/AppShell.tsx:79-81` (add pane)

**Interfaces:**
- Consumes: `useBreakpoints` (Task 7), `useProjects`, `Segmented`, `Button`, `Input`, `Select`.

- [ ] **Step 1: Write the failing test**

This task is UI wiring; verify via typecheck + a render-free smoke that the module exports a component. Create `src/components/BreakpointsView.test.tsx`:

```tsx
import { describe, it, expect } from "vitest";
import { BreakpointsView } from "./BreakpointsView";

describe("BreakpointsView", () => {
  it("is a component", () => {
    expect(typeof BreakpointsView).toBe("function");
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm exec vitest run src/components/BreakpointsView.test.tsx`
Expected: FAIL — cannot resolve `./BreakpointsView`.

- [ ] **Step 3: Write minimal implementation**

Create `src/components/BreakpointsView.tsx`:

```tsx
import { useEffect } from "react";
import { CircleDot, Plus, Save, Trash2 } from "lucide-react";
import { useBreakpoints, type Breakpoint } from "../breakpoints";
import { useProjects } from "../projects";
import { useState } from "react";
import { EmptyState } from "./EmptyState";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { Select } from "./ui/select";
import { cn } from "@/lib/utils";

const METHODS = ["*", "GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

export function BreakpointsView() {
  const { breakpoints, selectedId, intercept, load, select, upsert, remove, setIntercept } =
    useBreakpoints();
  const activeId = useProjects((s) => s.activeId);

  useEffect(() => {
    void load();
  }, [load]);

  const scoped = breakpoints.filter((b) => (b.projectId ?? null) === (activeId ?? null));

  const newBreakpoint = () => {
    void upsert({
      id: crypto.randomUUID(),
      name: "New breakpoint",
      enabled: true,
      pattern: "*/*",
      method: null,
      onRequest: true,
      onResponse: false,
      projectId: activeId ?? null,
    });
  };

  const selected = scoped.find((b) => b.id === selectedId) ?? null;

  return (
    <div className="flex h-full">
      <div className="flex w-64 shrink-0 flex-col border-r border-border">
        <div className="flex items-center gap-2 border-b border-border bg-card px-2 py-1.5">
          <span className="text-xs font-semibold text-muted-foreground">Breakpoints</span>
          <label className="ml-auto flex items-center gap-1 text-[11px] text-muted-foreground">
            <input
              type="checkbox"
              checked={intercept}
              onChange={(e) => void setIntercept(e.target.checked)}
            />
            intercept
          </label>
          <Button size="iconSm" variant="ghost" title="New breakpoint" onClick={newBreakpoint}>
            <Plus />
          </Button>
        </div>
        <div className="min-h-0 flex-1 overflow-auto">
          {scoped.map((b) => (
            <button
              key={b.id}
              onClick={() => select(b.id)}
              className={cn(
                "flex w-full items-center gap-2 px-3 py-2 text-left text-xs",
                b.id === selectedId ? "bg-primary/15" : "hover:bg-accent",
              )}
            >
              <CircleDot
                className={cn("size-3.5 shrink-0", b.enabled ? "text-http-red" : "text-muted-foreground")}
              />
              <span className="min-w-0 flex-1">
                <span className="block truncate font-medium">{b.name}</span>
                <span className="block truncate text-muted-foreground">{b.pattern}</span>
              </span>
            </button>
          ))}
          {scoped.length === 0 && (
            <div className="p-3 text-xs text-muted-foreground">No breakpoints yet — press ＋</div>
          )}
        </div>
      </div>

      <div className="min-w-0 flex-1">
        {selected ? (
          <BreakpointEditor
            key={selected.id}
            bp={selected}
            onSave={upsert}
            onDelete={() => void remove(selected.id)}
          />
        ) : (
          <EmptyState
            icon={<CircleDot className="size-8" />}
            title="Select a breakpoint"
            hint="A breakpoint pauses matching traffic so you can edit it live before it continues."
          />
        )}
      </div>
    </div>
  );
}

function BreakpointEditor({
  bp,
  onSave,
  onDelete,
}: {
  bp: Breakpoint;
  onSave: (b: Breakpoint) => Promise<void>;
  onDelete: () => void;
}) {
  const [draft, setDraft] = useState<Breakpoint>(bp);
  const patch = (p: Partial<Breakpoint>) => setDraft((d) => ({ ...d, ...p }));

  return (
    <div className="flex h-full flex-col">
      <div className="flex flex-wrap items-center gap-2 border-b border-border bg-card px-3 py-2">
        <Input
          value={draft.name}
          onChange={(e) => patch({ name: e.target.value })}
          className="h-7 w-44"
          placeholder="Name"
        />
        <Input
          value={draft.pattern}
          onChange={(e) => patch({ pattern: e.target.value })}
          className="h-7 w-56 font-mono"
          placeholder="host/path glob, e.g. api.example.com/*"
        />
        <Select
          value={draft.method ?? "*"}
          onChange={(e) => patch({ method: e.target.value === "*" ? null : e.target.value })}
        >
          {METHODS.map((m) => (
            <option key={m} value={m}>
              {m === "*" ? "any method" : m}
            </option>
          ))}
        </Select>
        <label className="flex items-center gap-1 text-xs text-muted-foreground">
          <input
            type="checkbox"
            checked={draft.onRequest}
            onChange={(e) => patch({ onRequest: e.target.checked })}
          />
          request
        </label>
        <label className="flex items-center gap-1 text-xs text-muted-foreground">
          <input
            type="checkbox"
            checked={draft.onResponse}
            onChange={(e) => patch({ onResponse: e.target.checked })}
          />
          response
        </label>
        <label className="flex items-center gap-1 text-xs text-muted-foreground">
          <input
            type="checkbox"
            checked={draft.enabled}
            onChange={(e) => patch({ enabled: e.target.checked })}
          />
          enabled
        </label>
        <div className="ml-auto flex items-center gap-1">
          <Button size="sm" onClick={() => void onSave(draft)}>
            <Save />
            Save
          </Button>
          <Button size="iconSm" variant="ghost" title="Delete" onClick={onDelete}>
            <Trash2 />
          </Button>
        </div>
      </div>
      <div className="p-3 text-xs text-muted-foreground">
        Matching {draft.onRequest && draft.onResponse ? "requests and responses" : draft.onRequest ? "requests" : draft.onResponse ? "responses" : "nothing (enable request or response)"} for
        <code className="mx-1 font-mono text-foreground">{draft.pattern}</code>
        {draft.method && draft.method !== "*" ? ` (${draft.method})` : ""} will pause in the Traffic view for live editing.
      </div>
    </div>
  );
}
```

In `src/components/TopBar.tsx`, extend the `Segmented` options:

```tsx
              options={[
                { value: "traffic", label: "Traffic" },
                { value: "rules", label: "Rules" },
                { value: "breakpoints", label: "Breakpoints" },
              ]}
```

In `src/components/AppShell.tsx`, import `BreakpointsView` and add a pane after the rules pane:

```tsx
          <Pane show={mode === "traffic" && view === "breakpoints"}>
            <BreakpointsView />
          </Pane>
```

(Add `import { BreakpointsView } from "./BreakpointsView";` near the other imports.)

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm exec vitest run src/components/BreakpointsView.test.tsx && pnpm exec tsc --noEmit`
Expected: PASS and no type errors.

- [ ] **Step 5: Commit**

```bash
git add src/components/BreakpointsView.tsx src/components/BreakpointsView.test.tsx src/components/TopBar.tsx src/components/AppShell.tsx
git commit -m "feat(breakpoints): management view, TopBar segment, AppShell pane"
```

---

## Task 10: InterceptEditor + FlowDetail integration (TS)

**Files:**
- Create: `src/components/InterceptEditor.tsx`
- Modify: `src/components/FlowDetail.tsx:92-100` (render InterceptEditor when paused)

**Interfaces:**
- Consumes: `useFlows().resolveBreakpoint` (Task 8), `bodyToText` (`@/lib/body`), `ScriptEditor` or a plain `<textarea>` for the body, `Button`, `Input`.
- Produces: `InterceptEditor({ flow }: { flow: Flow })`.

- [ ] **Step 1: Write the failing test**

Create `src/components/InterceptEditor.test.tsx`:

```tsx
import { describe, it, expect } from "vitest";
import { InterceptEditor } from "./InterceptEditor";

describe("InterceptEditor", () => {
  it("is a component", () => {
    expect(typeof InterceptEditor).toBe("function");
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm exec vitest run src/components/InterceptEditor.test.tsx`
Expected: FAIL — cannot resolve `./InterceptEditor`.

- [ ] **Step 3: Write minimal implementation**

Create `src/components/InterceptEditor.tsx`:

```tsx
import { useState } from "react";
import { Check, Reply, Ban } from "lucide-react";
import { useFlows } from "../store";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { bodyToText } from "@/lib/body";
import type { Flow, Header } from "@/types";

type Row = { key: string; value: string };

function toRows(headers: Header[]): Row[] {
  return headers.map(([key, value]) => ({ key, value }));
}
function toHeaders(rows: Row[]): [string, string][] {
  return rows.filter((r) => r.key.trim() !== "").map((r) => [r.key, r.value]);
}

export function InterceptEditor({ flow }: { flow: Flow }) {
  const phase = flow.pausedPhase ?? "request";
  const resolve = useFlows((s) => s.resolveBreakpoint);

  const isRequest = phase === "request";
  const [method, setMethod] = useState(flow.method);
  const [status, setStatus] = useState(String(flow.response?.status ?? 200));
  const [rows, setRows] = useState<Row[]>(
    toRows(isRequest ? flow.request.headers : flow.response?.headers ?? []),
  );
  const [body, setBody] = useState(bodyToText(isRequest ? flow.request : flow.response));
  const [busy, setBusy] = useState(false);

  const patchRow = (i: number, p: Partial<Row>) =>
    setRows((rs) => rs.map((r, j) => (j === i ? { ...r, ...p } : r)));
  const removeRow = (i: number) => setRows((rs) => rs.filter((_, j) => j !== i));
  const addRow = () => setRows((rs) => [...rs, { key: "", value: "" }]);

  const act = async (action: "execute" | "abort" | "respond") => {
    setBusy(true);
    try {
      if (action === "abort") {
        await resolve(flow.id, phase, "abort", { reason: "aborted from UI" });
      } else if (action === "respond") {
        await resolve(flow.id, phase, "respond", {
          status: Number(status) || 200,
          headers: toHeaders(rows),
          body,
        });
      } else {
        await resolve(flow.id, phase, "execute", {
          method: isRequest ? method : undefined,
          status: isRequest ? undefined : Number(status) || 200,
          headers: toHeaders(rows),
          body,
        });
      }
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-2 border-b border-border bg-http-red/10 px-3 py-2">
        <span className="rounded bg-http-red px-1.5 py-0.5 text-[10px] font-semibold uppercase text-white">
          Paused · {phase}
        </span>
        <div className="ml-auto flex items-center gap-1">
          <Button size="sm" disabled={busy} onClick={() => void act("execute")}>
            <Check />
            Execute
          </Button>
          {isRequest && (
            <Button size="sm" variant="outline" disabled={busy} onClick={() => void act("respond")}>
              <Reply />
              Respond locally
            </Button>
          )}
          <Button size="sm" variant="destructive" disabled={busy} onClick={() => void act("abort")}>
            <Ban />
            Abort
          </Button>
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-auto p-3">
        <div className="mb-3 flex items-center gap-2">
          {isRequest ? (
            <label className="flex items-center gap-1 text-xs text-muted-foreground">
              Method
              <Input value={method} onChange={(e) => setMethod(e.target.value)} className="h-7 w-28 font-mono" />
            </label>
          ) : (
            <label className="flex items-center gap-1 text-xs text-muted-foreground">
              Status
              <Input value={status} onChange={(e) => setStatus(e.target.value)} className="h-7 w-24 font-mono" />
            </label>
          )}
        </div>

        <div className="mb-1 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
          Headers
        </div>
        <table className="mb-3 w-full border-collapse text-xs">
          <tbody>
            {rows.map((r, i) => (
              <tr key={i} className="border-b border-border/50">
                <td className="w-1/3 py-1 pr-2">
                  <Input
                    value={r.key}
                    onChange={(e) => patchRow(i, { key: e.target.value })}
                    className="h-6 font-mono"
                  />
                </td>
                <td className="py-1 pr-2">
                  <Input
                    value={r.value}
                    onChange={(e) => patchRow(i, { value: e.target.value })}
                    className="h-6 font-mono"
                  />
                </td>
                <td className="w-8 py-1">
                  <Button size="iconSm" variant="ghost" title="Remove" onClick={() => removeRow(i)}>
                    ×
                  </Button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        <Button size="sm" variant="ghost" onClick={addRow}>
          + Add header
        </Button>

        <div className="mb-1 mt-3 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
          Body
        </div>
        <textarea
          value={body}
          onChange={(e) => setBody(e.target.value)}
          spellCheck={false}
          className="h-64 w-full resize-y rounded border border-border bg-card p-2 font-mono text-xs"
        />
      </div>
    </div>
  );
}
```

In `src/components/FlowDetail.tsx`, at the top of `FlowDetail` after the `if (!flow) { ... }` guard, add:

```tsx
  if (flow.pausedPhase) {
    return <InterceptEditor flow={flow} />;
  }
```

and add the import: `import { InterceptEditor } from "./InterceptEditor";`.

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm exec vitest run src/components/InterceptEditor.test.tsx && pnpm exec tsc --noEmit`
Expected: PASS and no type errors. (Confirm `bodyToText` accepts `HttpMessage | ResponseMessage | null` — it is used that way in FlowDetail already.)

- [ ] **Step 5: Commit**

```bash
git add src/components/InterceptEditor.tsx src/components/InterceptEditor.test.tsx src/components/FlowDetail.tsx
git commit -m "feat(breakpoints): live editor for paused flows"
```

---

## Task 11: Document `ctx.breakpoint()` for autocomplete (TS)

**Files:**
- Modify: `src/scripting/apiTypes.ts:37-48` (add to `TrawlCtx`)
- Modify: `src/scripting/stdlib.ts:62-80` (add to `STD_FUNCTIONS`)

**Interfaces:**
- Consumes: nothing new. Pure documentation/autocomplete.

- [ ] **Step 1: Write the failing test**

Add a guard test. Create `src/scripting/apiTypes.test.ts`:

```ts
import { describe, it, expect } from "vitest";
import { API_DTS } from "./apiTypes";

describe("API_DTS", () => {
  it("documents ctx.breakpoint()", () => {
    expect(API_DTS).toContain("breakpoint(");
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm exec vitest run src/scripting/apiTypes.test.ts`
Expected: FAIL — `API_DTS` has no `breakpoint(`.

- [ ] **Step 3: Write minimal implementation**

In `src/scripting/apiTypes.ts`, inside `interface TrawlCtx`, after the `abort(reason?: string): void;` line add:

```ts
  /**
   * Pause the flow on a breakpoint: it is held in-flight and surfaced in the
   * Traffic view for live editing until you Execute, Respond, or Abort it.
   * Works in the request and response phases.
   */
  breakpoint(): void;
```

In `src/scripting/stdlib.ts`, add an entry to the `STD_FUNCTIONS` array (after the `queryParam` entry):

```ts
  { signature: "ctx.breakpoint()", doc: "Pause the flow for live editing in the Traffic view." },
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm exec vitest run src/scripting/apiTypes.test.ts && pnpm exec tsc --noEmit`
Expected: PASS and no type errors.

- [ ] **Step 5: Commit**

```bash
git add src/scripting/apiTypes.ts src/scripting/stdlib.ts src/scripting/apiTypes.test.ts
git commit -m "docs(breakpoints): ctx.breakpoint() autocomplete & reference"
```

---

## Task 12: Full verification & manual smoke

**Files:** none (verification only).

- [ ] **Step 1: Run the whole test suite**

Run: `pnpm exec tsc --noEmit && pnpm exec vitest run && cd src-tauri && cargo test`
Expected: all green.

- [ ] **Step 2: Manual smoke (documented, run by a human)**

Document in the PR description these manual checks (run `pnpm tauri dev`):
1. Breakpoints view → add a breakpoint `*/*`, request only → make a request through the proxy → it appears Paused and auto-selected → edit a header → Execute → upstream receives the edit.
2. Enable response on the breakpoint → make a request → response pauses → edit status to 418 → Execute → client sees 418.
3. Request breakpoint → Respond locally with a body → client gets it, upstream not hit.
4. Abort → client gets 502.
5. A rule with `ctx.breakpoint()` (request phase) pauses the flow.
6. Toggle `intercept` off → traffic flows without pausing.

- [ ] **Step 3: Finish the branch**

Use the `superpowers:finishing-a-development-branch` skill to open a PR (branch `feat/interactive-breakpoints`).

---

## Self-Review Notes

- **Spec coverage:** core hold mechanism (Task 4/5), UI list trigger (Task 1/6/9), `ctx.breakpoint()` trigger (Task 3/4/5/11), full request edit + Execute/Abort/Respond (Task 4/10), response edit + Execute/Abort (Task 5/10), `paused_phase` model + `flow-paused` event (Task 2/4/8), commands (Task 6), management view + intercept toggle + paused count indicator (Task 9), paused editor (Task 10), no-DB-persist-while-paused (Task 4/5), hold-indefinitely + drop-safe (Task 4), binary bodies pass through (Task 4/5 `is_text` guards). Covered.
- **Type consistency:** `Resolution`/`BpPhase`/`BreakpointRegistry` defined in Task 4, reused verbatim in Task 5/6. `EditedPayload` (Rust) fields match the frontend `EditedPayload` (Task 8) and `resolve_breakpoint` args. `Breakpoint` camelCase (`onRequest`/`onResponse`/`projectId`) consistent between Rust serde (Task 1) and TS (Task 7).
- **Paused count indicator:** derived in components via `useFlows((s) => s.flows.filter(f => f.state === "paused").length)` — no store field needed; add it to the TopBar/StatusBar opportunistically in Task 9 if desired (optional, not gated).
