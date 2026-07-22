# Notifications plugin (Telegram) + core secrets — design

Date: 2026-07-22
Status: approved

## Goal

A new **notifications plugin** (`trawl-plugin-notifications`, separate sibling
repo) that can subscribe to any event on the plugin bus — core events or events
emitted by other plugins — and deliver messages to **Telegram**. Payload
autocomplete ("подсказки по пейлоду") in the subscription editor. Rule scripts
gain a `notify()` function. The core gains a **secrets** store backed by the
macOS Keychain (e.g. for the bot token).

Core changes live in `http-catch`; delivery logic lives entirely in the plugin.
The core never knows about Telegram — new channels (Slack, etc.) are just new
plugins listening to the same events.

## 1. Core: secrets

New module `src-tauri/src/secrets.rs` using the `keyring` crate:

- Values stored in the macOS Keychain — service `trawl`, account = secret name.
- The **list of names** is kept in the regular store (`store.rs`), because the
  Keychain cannot enumerate entries.
- Tauri commands: `secrets_list() -> Vec<String>`, `secret_get(name)`,
  `secret_set(name, value)`, `secret_delete(name)`.

Scope: **global named secrets** — one app-wide `name → value` list, not
per-plugin, not per-project.

Consumers:

- **Settings UI**: a "Secrets" section — list names, add/edit/delete, values
  masked (write-only display; reveal on demand is out of scope).
- **Plugin API** (`window.__TRAWL__`), API version bump to **1.6.0**:

  ```ts
  interface TrawlSecrets {
    list(): Promise<string[]>;
    get(name: string): Promise<string | null>;
    set(name: string, value: string): Promise<void>;
    remove(name: string): Promise<void>;
  }
  ```

- **Rule scripts**: a native `secret(name: string): string | null` function,
  bound into the QuickJS engine in all phases (request/response engine thread
  and the handler runtime). Reads the Keychain synchronously on the engine
  thread. Declared in `src/scripting/stdlib.ts` for Monaco autocomplete and the
  Function-library reference.

## 2. Core: event registry + payload hints

The plugin bus (`src/plugins/bus.ts`) is extended:

- `host.events.describe(type, { description?, dts?, source? })` — declare an
  event and the TS type of its payload (a `.d.ts` source string, consistent
  with the existing Monaco approach). `source` is self-reported by the caller
  (the bus cannot attribute callers — plugins share one context); the core
  passes `"core"`. The core declares its own events this way:
  `flow:added`, `flow:updated`, `capture:started`, `capture:stopped`,
  `filter:changed`, `project:changed`, and the new `notify:send`.
- The bus remembers the **last payload of every event type** that passes
  through it (declared or not).
- `host.events.known(): EventInfo[]` returns
  `{ type, description?, dts?, lastPayload?, source?: "core" | pluginId }[]` —
  the union of declared events and observed-only events.

Hints strategy: declared `dts` wins; for undeclared events the payload
structure is **inferred from the last observed payload** using the same
structure-inference mechanism already used for response autocomplete in rules
(`src/scripting/apiTypes.ts`).

## 3. Core: `notify()` in the scripting engine

Rule scripts (all phases) gain:

```ts
notify(text: string, opts?: { channel?: string; title?: string }): void
```

- Implementation: `notify()` pushes into `ctx.__notifications`; the array is
  returned as a new `ScriptResult.notifications` field.
- After a rule runs, the backend emits a Tauri event; the frontend host bridges
  it onto the plugin bus as **`notify:send`** with payload
  `{ text, channel?, title?, source: "rule", ruleName?, flowId? }`.
- Plugins (or plugin UIs) can also emit `notify:send` directly on the bus —
  the event is the single delivery contract.

## 4. Plugin: `trawl-plugin-notifications`

New sibling repository following the standard plugin scaffold (manifest, Vite
IIFE build, `apiVersion: "1.6.0"`). Registers a **Notifications** mode with
three parts:

### Channels

Named channel list: `{ name, type: "telegram", tokenSecretName, chatId }`.

- The bot token is stored in core secrets; the channel references it **by
  secret name**.
- "Send test" button per channel.
- `type` is an enum with a single value for now — future channel kinds slot in.

### Subscriptions

`{ id, event, channel, template, condition?, throttleSec?, enabled }`.

- **Event**: dropdown fed by `host.events.known()`, with free-text input for
  events that have not been seen yet.
- **Template**: text with `{{payload.expr}}` placeholders — each placeholder is
  a JS expression evaluated against the event payload.
- **Condition**: optional JS expression; falsy result → no message.
- **Throttle** (`throttleSec`): optional minimum interval between sends for
  this subscription — protection against chatty events like `flow:added`.
- Template and condition are edited in Monaco with payload autocomplete from
  the registry `dts` or the inferred last-payload structure.

### Delivery & log

- For each enabled subscription: `host.events.on(event)` → condition → render
  template → `host.http.send` to
  `https://api.telegram.org/bot<token>/sendMessage` with the channel's
  `chat_id`.
- `notify:send` is handled specially: the text is already rendered; the channel
  comes from `opts.channel`, falling back to the first configured channel.
- A **log** of recent notifications (sent/failed, timestamp, subscription)
  persisted via `host.storage`, shown in the mode UI.

## 5. Error handling

- Missing secret / missing channel / Telegram API error → the attempt is
  recorded in the log as failed with the error text; no retries (YAGNI).
- `notify()` in a script never throws — collection is passive; delivery
  failures surface only in the plugin log.
- Keychain access errors surface as command errors in the Secrets UI.

## 6. Testing

- **Bus registry** (vitest): describe/known/last-payload behaviour.
- **Secrets** (Rust): commands against a mock keyring (`keyring` mock/in-memory
  credential store) so CI needs no real Keychain.
- **Scripting** (Rust): `notify()` populates `ScriptResult.notifications`;
  `secret()` binding returns values from the mock store.
- **Plugin** (vitest): template rendering, condition evaluation, throttling.

## 7. Process & docs

- Core work happens in an isolated **git worktree**, merged to `main` when done
  (per the established workflow). The plugin is a brand-new repo — no worktree
  needed.
- `docs/plugins.md` gains: `host.secrets`, `events.describe`/`events.known`,
  the `notify:send` event, and the `notify()`/`secret()` script functions.
