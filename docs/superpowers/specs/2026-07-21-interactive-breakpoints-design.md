# Interactive Breakpoints — Design

**Date:** 2026-07-21
**Status:** Approved (design), pending implementation plan

## Goal

Add interactive traffic interception ("breakpoints") to Trawl: hold a matching
request or response in-flight, surface it in the UI, let the user edit it live,
then continue — as Charles, Proxyman, and Burp Suite do. Editing covers **both**
the request (method, URL, headers, body) and the response (status, headers, body),
depending on which phase was intercepted.

Breakpoints can be armed two ways:

1. **From the UI** — a dedicated list of breakpoint definitions (pattern + method +
   request/response toggles), mirroring the existing Rules list.
2. **From a rule/script** — a new `ctx.breakpoint()` function, alongside the
   existing `ctx.mock()` / `ctx.abort()`.

## Current state (what we build on)

- The proxy handler (`src-tauri/src/proxy.rs`) runs `handle_request` /
  `handle_response`, both `async`, and already `.await`s script runs. This is the
  seam we hang a flow on.
- Rules and the script library live in `Arc<RwLock<…>>` shared with the proxy and
  are persisted as JSON (`rules.json`) / text (`library.js`) under the app data
  `scripting/` dir. Breakpoint definitions follow the same pattern
  (`breakpoints.json`).
- `FlowState::Paused` already exists in the enum (`model.rs`) but nothing sets it.
- The script engine (`scripting.rs`) returns a `ScriptResult { action, … }`;
  `ctx.mock()` and `ctx.abort()` set `__action`. `ctx.breakpoint()` adds a new
  action the async handler acts on.
- Scoping: an active project restricts which rules apply and which hosts are
  recorded. Breakpoints reuse `active_scope()` and `glob_to_regex` matching.

## Core mechanism: holding a flow

When the proxy decides to break on a flow:

1. Set `flow.state = Paused` and `flow.paused_phase = Some("request" | "response")`.
2. Emit a `flow-paused` event carrying the `Flow` (its `request` / `response`
   holds the current working copy the UI edits).
3. Create a `tokio::sync::oneshot` channel; store the sender in a shared registry:
   ```rust
   type BreakpointRegistry = Arc<Mutex<HashMap<(u64 /*flow id*/, Phase), oneshot::Sender<Resolution>>>>;
   ```
4. `.await` the receiver.
5. The UI resolves via a Tauri command → the sender fires → the handler applies
   the resolution and continues.

```rust
enum Resolution {
    Execute(EditedMessage), // continue with edits applied
    Abort(String),          // short-circuit with 502
    Respond(MockSpec),      // request phase only: local response, never hits upstream
}
```

`EditedMessage` for the request phase carries method, url, headers, body; for the
response phase carries status, headers, body.

Timeout behaviour: **hold indefinitely** until the user acts. (The client may
time out on its own; that is acceptable and matches Charles/Burp defaults.) If the
proxy stops or the app shuts down, pending senders are dropped, the `.await`
resolves to `Err`, and the handler falls back to continuing unmodified so no
connection hangs forever on our side.

Rejected alternative: a poll loop that sets `Paused` and re-reads a shared
resolution map with `sleep`. Simpler to picture but wastes CPU and adds latency;
the oneshot fits the existing async path cleanly.

## Trigger 1: UI breakpoint list

New persisted type, stored in `scripting/breakpoints.json`:

```rust
struct Breakpoint {
    id: String,
    name: String,
    enabled: bool,
    pattern: String,          // host/path glob, e.g. "api.example.com/*" — reuses glob_to_regex
    method: Option<String>,   // None or "*" = any method
    on_request: bool,
    on_response: bool,
    project_id: Option<String>, // None = global; matched via active_scope()
}
```

Matching mirrors `Rule`: enabled, in the active project's scope, pattern matches
one of the `host/path` targets, and (for the method filter) the request method
matches. A breakpoint fires in the request phase when `on_request` is set, in the
response phase when `on_response` is set.

## Trigger 2: `ctx.breakpoint()` from a rule

- In `scripting.rs`, the request/response wrapper gains
  `ctx.breakpoint = function() { ctx.__action = "breakpoint"; }`.
- `ScriptResult.action` can now be `"breakpoint"`.
- In `proxy.rs`, when a request-phase or response-phase rule returns
  `action == "breakpoint"`, the handler pauses using the **current** working
  headers/body (i.e. edits earlier rules already made are preserved as the
  snapshot the user sees).
- Add the declaration to `src/scripting/apiTypes.ts` (Monaco autocomplete) and a
  documented entry to `src/scripting/stdlib.ts` so it appears in the function list.

Both triggers converge on the same pause/registry/await path.

## Model & events

- `Flow` gains `paused_phase: Option<String>` (serde `default`, camelCase
  `pausedPhase`), values `"request"` / `"response"` / absent. This tells the UI
  which side to edit and lets the list show a "paused" indicator.
- Paused flows are **not** persisted to SQLite; `persist()` runs only once the
  flow is resolved (completed / errored), as today.
- Events: new `flow-paused` (payload = `Flow` with `state=paused`,
  `pausedPhase` set). On resolution the handler emits the existing `flow-updated`.

## Tauri commands

Mirror the rules commands for definitions, plus resolution:

- `list_breakpoints() -> Vec<Breakpoint>`
- `save_breakpoint(breakpoint) -> Vec<Breakpoint>`
- `delete_breakpoint(id) -> Vec<Breakpoint>`
- `resolve_breakpoint(id: u64, phase: "request"|"response", action: "execute"|"abort"|"respond", edited: EditedPayload)`
  — looks up `(id, phase)` in the registry and sends the matching `Resolution`.
  `edited` carries the modified message; for `respond` it carries the local
  response spec (status/headers/body).

Register all in `lib.rs`. The `BreakpointRegistry` and `SharedBreakpoints` live on
`AppState` and are threaded into `proxy::start` like `rules` / `library` /
`active_project` are today.

## UI

### Breakpoints management view
- Add a third `Segmented` option `Breakpoints` in `TopBar.tsx` next to
  Traffic / Rules (extend the `View` union in `store.ts`).
- New `BreakpointsView.tsx` styled like `RulesView`: a left list of definitions,
  an editor on the right with name, pattern, method select, Request/Response
  toggles, enabled checkbox, save/delete. Scoped to the active project.
- Header includes a global **Intercept on/off** switch (disables all breakpoints
  without deleting them) and a count of currently paused flows.
- New `breakpoints.ts` zustand store (load/upsert/remove) mirroring `rules.ts`.

### Paused-flow editor
- When the selected flow has `pausedPhase` set, `FlowDetail.tsx` renders an
  **editable** view instead of the read-only one:
  - request phase: method input, URL input, editable headers table, Monaco body
    editor; actions **Execute**, **Abort**, **Respond locally**.
  - response phase: status input, editable headers table, Monaco body editor;
    actions **Execute**, **Abort**.
- Extracted into an `InterceptEditor.tsx` component. Editable headers reuse the
  shape of `HeadersTable` with add/remove/edit affordances.
- A newly paused flow is auto-selected. Multiple paused flows form a queue,
  resolved one at a time; the list shows a paused badge and count.
- `store.ts` listens for `flow-paused` (upsert) and gains a `resolveBreakpoint`
  action that invokes `resolve_breakpoint` and optimistically clears the local
  paused state.

## Testing

### Rust (via the existing proxy test harness in `proxy.rs`)
- Request breakpoint matches → a spawned task calls `resolve_breakpoint` with an
  edited header/body → the edit reaches the upstream echo server.
- Response breakpoint matches → resolve with an edited status/body → the client
  receives the edited response.
- `Abort` resolution → client gets 502.
- `Respond` resolution (request phase) → client gets the local body; upstream is
  never contacted (point at a dead address, as `request_rule_mock_short_circuits`
  does).
- `ctx.breakpoint()` in a request rule pauses the flow (registry gains an entry);
  resolving continues it.
- Breakpoint matching unit tests (pattern + method + scope), mirroring the `Rule`
  matching tests.

### Frontend (vitest)
- `breakpoints.ts` store: load / upsert / remove round-trips.
- `store.ts`: `flow-paused` upsert sets paused state; `resolveBreakpoint` clears it.
- Definition matching / project scoping helper (if any pure logic is extracted).

## Files touched

**Rust (`src-tauri/src/`):**
- new `breakpoints.rs` — `Breakpoint` type, load/save, matching.
- `proxy.rs` — registry, pause/await in both phases, `ctx.breakpoint` action,
  resolution application.
- `scripting.rs` — `ctx.breakpoint()`, `"breakpoint"` action.
- `model.rs` — `paused_phase` field.
- `commands.rs` — breakpoint CRUD + `resolve_breakpoint`, `AppState` wiring.
- `lib.rs` — register commands.

**Frontend (`src/`):**
- new `breakpoints.ts` (store), `components/BreakpointsView.tsx`,
  `components/InterceptEditor.tsx`.
- `types.ts` — `pausedPhase` on `Flow`.
- `store.ts` — `View` union, `flow-paused` listener, `resolveBreakpoint`.
- `components/TopBar.tsx` — Breakpoints segment + intercept toggle/count.
- `components/AppShell.tsx` — mount the Breakpoints pane.
- `components/FlowDetail.tsx` — switch to `InterceptEditor` when paused.
- `scripting/apiTypes.ts`, `scripting/stdlib.ts` — document `ctx.breakpoint()`.

## Out of scope (YAGNI)

- Configurable auto-continue timeout (decided: hold indefinitely).
- A global "intercept everything" mode with no matching (the global toggle only
  enables/disables the defined breakpoints).
- Persisting or replaying paused flows across app restarts.
- Editing raw bytes for non-text bodies (edit text bodies; binary bodies pass
  through unedited, consistent with how rules treat them today).
