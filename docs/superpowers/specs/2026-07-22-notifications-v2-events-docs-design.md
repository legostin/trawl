# Notifications v2 — more events, event docs, native editor UX — design

Date: 2026-07-22
Status: approved
Builds on: `2026-07-22-notifications-design.md` (shipped as core 0.5.0 / host API 1.6.0 / plugin 0.1.0)

## Goal

Round out the eventing story and the notifications plugin UX:

1. **More core events** — breakpoints, rule outcomes, flow errors, plugin
   lifecycle, app updates.
2. **Structured event documentation** — every declared event carries per-field
   parameter docs, surfaced in the subscription editor UI.
3. **In-app hints** — the plugin UI explains itself (tokens, chat_id, template
   syntax, conditions, throttle).
4. **Native editor UX** — host's Button/Input/Select exposed to plugins
   (uniform control heights), Monaco for the template (handlebars), Examples
   menus for template and condition.

## 1. New events

| Bus event | Origin | Payload |
|---|---|---|
| `breakpoint:hit` | bridge of existing Tauri `flow-paused` | Flow (with `pausedPhase`) |
| `breakpoint:resolved` | new Tauri `flow-resumed`, emitted wherever a paused flow resumes via Execute/Respond/Abort | Flow |
| `breakpoint:timeout` | new Tauri `breakpoint-timeout`, emitted when auto-continue fires (`await_resolution` timeout branch) | Flow |
| `rule:applied` | new Tauri `rule-applied` at the three rule-application sites (request/response/handler) | `{ ruleName, phase, flowId, method, host, path }` |
| `rule:error` | new Tauri `rule-error` where a rule script returns action "error" | same + `error` |
| `flow:error` | frontend derivation: first `flow:added`/`flow:updated` with `state === "error"` per flow id (capped seen-set) | Flow |
| `plugin:installed` / `plugin:removed` | frontend `plugins.ts` lifecycle | `{ id, name?, version? }` |
| `update:available` | frontend `updater.ts` when a newer release is found | `{ version, notes? }` |

Rejected for now (YAGNI): `report:saved`, `env:changed`, `capture:error`.

**Backend plumbing.** The proxy's `NotifyFn` generalizes to
`AppEventFn = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>` so one
channel carries `script-notify`, `rule-applied` and `rule-error` (the
Flow-payload events reuse the existing `EmitFn`). `commands.rs` wires it to
`app.emit(event, payload)`.

## 2. Event docs with parameters

- `EventMeta` gains `params?: { name: string; type: string; doc?: string }[]`.
  Core describes every event (old and new) with per-field docs.
- Subscription editor gains a **doc panel** for the selected event:
  description + parameter table (name, type, doc, live example value from
  `lastPayload` where available). Clicking a parameter inserts
  `{{payload.<path>}}` into the template.
- The event dropdown shows each event's description.

## 3. Hints across the plugin UI

Short helper texts/tooltips: how to get a bot token (@BotFather) and chat_id
(@userinfobot), what a token-secret reference is, `{{…}}` template syntax,
what condition and throttle do. Empty states explain the next step.

## 4. Native editor UX

- `host.ui` additionally exposes the host's **Button, Input, Select**; the
  plugin uses them everywhere (fixes inconsistent control heights).
- Template edited in `host.ui.ScriptEditor` with `language="handlebars"`
  (bundled Monaco language, `{{…}}` highlighting); condition stays
  `javascript`.
- **Examples** menus (SnippetMenu-style dropdown) above template and
  condition: ready-made variants keyed to the selected event (5xx alert,
  breakpoint hit, rule applied/failed, flow error with cURL-ish summary,
  update available…). Examples data lives in the plugin.

## Versions & process

- Host API → **1.7.0**; app → **0.6.0**; plugin → **0.2.0** (apiVersion 1.7.0).
- `docs/plugins.md`: new events table rows, `params` in the registry section,
  new `ui` components.
- Core work in a git worktree merged to `main`; plugin in its repo, pushed to
  GitHub after completion (repo is already published; catalog entry unchanged).
- Testing: Rust tests for the new proxy emits (AppEventFn capture); bus test
  for `params` passthrough; plugin tests for examples data integrity and the
  param-insert path rewrite; existing suites stay green.
