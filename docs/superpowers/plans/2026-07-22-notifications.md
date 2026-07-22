# Notifications Plugin (Telegram) + Core Secrets Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add app-wide Keychain-backed secrets to the core, an event registry with payload hints, a `notify()`/`secret()` script API, and a new `trawl-plugin-notifications` plugin that delivers bus events to Telegram.

**Architecture:** The core (repo `http-catch`) gains: a `secrets.rs` module (keyring/Keychain), `secret()` + `notify()` in the QuickJS script engine, a `NotifyFn` threaded through the proxy that emits a Tauri `script-notify` event, and an event registry on the plugin bus (`describe`/`known` + last-payload capture). The frontend host bridges `script-notify` → bus event `notify:send`. Delivery lives entirely in a new sibling plugin repo `trawl-plugin-notifications` that subscribes to bus events, renders templates, and posts to the Telegram Bot API via `host.http.send`.

**Tech Stack:** Rust (tauri 2, keyring 3, rquickjs 0.9, tokio), TypeScript/React 19, Vite IIFE plugin bundle, vitest, Monaco (host-provided `ScriptEditor`).

**Spec:** `docs/superpowers/specs/2026-07-22-notifications-design.md`

## Global Constraints

- Core work happens in an **isolated git worktree** of `http-catch` (per `superpowers:using-git-worktrees`), merged to `main` at the end.
- The plugin is a **new repo** at `/Users/legostin/claude-projects/trawl-plugin-notifications` (git init, no worktree).
- Host API version bumps to **`1.6.0`** (`HOST_VERSION` in `src/plugins/host.ts`, `apiVersion` in the plugin manifest).
- App version bumps to **0.5.0** (0.4.0 is already taken by the plugin-catalog release) (`package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`).
- Secrets scope: **global named**, Keychain service **`trawl`**, account = secret name. Names index in `<app-data>/secrets.json` (Keychain cannot enumerate).
- Deviation from spec noted during planning: the registry field is named **`payloadType`** (a TS *type expression*, e.g. `"{ a: number }"`), not a full `.d.ts` string — one mechanism serves both declared and inferred types (`fieldsToType` output).
- Rust commands run from `src-tauri/`: `cargo test`. Frontend: `pnpm test` (vitest), `pnpm build` (tsc + vite). Plugin repo: `pnpm test`, `pnpm build`.
- Do not commit `src-tauri/Cargo.lock` noise unrelated to the keyring addition; commit it together with the `Cargo.toml` change.

---

## Part A — core (`http-catch`, in the worktree)

### Task 1: Secrets backend (Rust)

**Files:**
- Modify: `src-tauri/Cargo.toml` (add keyring)
- Create: `src-tauri/src/secrets.rs`
- Modify: `src-tauri/src/lib.rs` (add `mod secrets;` + register 4 commands)

**Interfaces:**
- Produces: `secrets::get(name: &str) -> anyhow::Result<Option<String>>`, `secrets::set(data_dir: &Path, name: &str, value: &str) -> anyhow::Result<()>`, `secrets::delete(data_dir: &Path, name: &str) -> anyhow::Result<()>`, `secrets::list_names(data_dir: &Path) -> Vec<String>`; Tauri commands `secrets_list`, `secret_get`, `secret_set`, `secret_delete`.

- [ ] **Step 1: Add the keyring dependency**

In `src-tauri/Cargo.toml` under `[dependencies]` add:

```toml
keyring = { version = "3", features = ["apple-native"] }
```

- [ ] **Step 2: Write `secrets.rs` with failing tests first**

Create `src-tauri/src/secrets.rs`. Write the test module and stub functions (`todo!()` bodies) so the tests compile and fail:

```rust
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use keyring::Entry;
use tauri::{AppHandle, Manager};

/// Keychain service name for all Trawl secrets.
const SERVICE: &str = "trawl";

fn index_path(data_dir: &Path) -> PathBuf {
    data_dir.join("secrets.json")
}

/// Names of stored secrets. The Keychain cannot enumerate entries, so an
/// index of names lives next to the other app data.
pub fn list_names(data_dir: &Path) -> Vec<String> {
    todo!()
}

fn save_names(data_dir: &Path, names: &[String]) -> Result<()> {
    todo!()
}

pub fn get(name: &str) -> Result<Option<String>> {
    todo!()
}

pub fn set(data_dir: &Path, name: &str, value: &str) -> Result<()> {
    todo!()
}

pub fn delete(data_dir: &Path, name: &str) -> Result<()> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// All tests run against keyring's in-memory mock store — no real Keychain.
    fn mock_store() {
        use std::sync::Once;
        static ONCE: Once = Once::new();
        ONCE.call_once(|| {
            keyring::set_default_credential_builder(keyring::mock::default_credential_builder())
        });
    }

    fn tmp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("trawl-secrets-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn set_get_roundtrip_and_index() {
        mock_store();
        let dir = tmp_dir("roundtrip");
        set(&dir, "TG_BOT_TOKEN", "12345:abc").unwrap();
        assert_eq!(get("TG_BOT_TOKEN").unwrap().as_deref(), Some("12345:abc"));
        assert_eq!(list_names(&dir), vec!["TG_BOT_TOKEN".to_string()]);
        // Overwrite keeps a single index entry.
        set(&dir, "TG_BOT_TOKEN", "67890:def").unwrap();
        assert_eq!(get("TG_BOT_TOKEN").unwrap().as_deref(), Some("67890:def"));
        assert_eq!(list_names(&dir).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_secret_is_none() {
        mock_store();
        assert_eq!(get("TRAWL_TEST_MISSING").unwrap(), None);
    }

    #[test]
    fn delete_removes_value_and_name() {
        mock_store();
        let dir = tmp_dir("delete");
        set(&dir, "TRAWL_TEST_DEL", "1").unwrap();
        delete(&dir, "TRAWL_TEST_DEL").unwrap();
        assert_eq!(get("TRAWL_TEST_DEL").unwrap(), None);
        assert!(list_names(&dir).is_empty());
        // Deleting a missing secret is not an error.
        delete(&dir, "TRAWL_TEST_DEL").unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_name_rejected() {
        mock_store();
        let dir = tmp_dir("empty");
        assert!(set(&dir, "  ", "x").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

Add `mod secrets;` to `src-tauri/src/lib.rs` next to the other `mod` declarations so the file compiles.

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test secrets -- --nocapture` (from `src-tauri/`)
Expected: panics with `not yet implemented` (the `todo!()` bodies).

- [ ] **Step 4: Implement the module**

Replace the `todo!()` bodies:

```rust
pub fn list_names(data_dir: &Path) -> Vec<String> {
    std::fs::read_to_string(index_path(data_dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_names(data_dir: &Path, names: &[String]) -> Result<()> {
    std::fs::create_dir_all(data_dir).context("create data dir")?;
    std::fs::write(index_path(data_dir), serde_json::to_string_pretty(names)?)
        .context("write secrets.json")?;
    Ok(())
}

pub fn get(name: &str) -> Result<Option<String>> {
    match Entry::new(SERVICE, name)?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn set(data_dir: &Path, name: &str, value: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        anyhow::bail!("secret name is empty");
    }
    Entry::new(SERVICE, name)?.set_password(value)?;
    let mut names = list_names(data_dir);
    if !names.iter().any(|n| n == name) {
        names.push(name.to_string());
        names.sort();
        save_names(data_dir, &names)?;
    }
    Ok(())
}

pub fn delete(data_dir: &Path, name: &str) -> Result<()> {
    match Entry::new(SERVICE, name)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => {}
        Err(e) => return Err(e.into()),
    }
    let names: Vec<String> = list_names(data_dir).into_iter().filter(|n| n != name).collect();
    save_names(data_dir, &names)
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test secrets` (from `src-tauri/`)
Expected: `4 passed`.

- [ ] **Step 6: Add Tauri commands and register them**

Append to `src-tauri/src/secrets.rs`:

```rust
fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn secrets_list(app: AppHandle) -> Result<Vec<String>, String> {
    Ok(list_names(&data_dir(&app)?))
}

#[tauri::command]
pub fn secret_get(name: String) -> Result<Option<String>, String> {
    get(&name).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn secret_set(app: AppHandle, name: String, value: String) -> Result<(), String> {
    set(&data_dir(&app)?, &name, &value).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn secret_delete(app: AppHandle, name: String) -> Result<(), String> {
    delete(&data_dir(&app)?, &name).map_err(|e| e.to_string())
}
```

In `src-tauri/src/lib.rs`, inside `tauri::generate_handler![ ... ]` after the `plugins::*` entries add:

```rust
            secrets::secrets_list,
            secrets::secret_get,
            secrets::secret_set,
            secrets::secret_delete,
```

- [ ] **Step 7: Full Rust build + tests**

Run: `cargo test` (from `src-tauri/`)
Expected: all tests pass, no warnings about unused code in `secrets.rs`.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/secrets.rs src-tauri/src/lib.rs
git commit -m "feat(secrets): Keychain-backed named secrets + tauri commands"
```

---

### Task 2: `secret()` and `notify()` in the script engine

**Files:**
- Modify: `src-tauri/src/scripting.rs`
- Modify: `src-tauri/src/commands.rs` (AppState::new — real secret resolver)
- Modify: `src-tauri/src/proxy.rs` (compile fixes at 2 call sites only)

**Interfaces:**
- Consumes: `crate::secrets::get` (Task 1).
- Produces: `pub type SecretFn = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>`; `spawn_engine(timeout: Duration, secrets: SecretFn) -> ScriptClient`; `execute_handler(prelude: &str, script: &str, input_json: &str, js_timeout: Duration, secrets: SecretFn) -> ScriptResult`; `ScriptResult.notifications: Vec<serde_json::Value>`; script globals `secret(name)`, `notify(text, opts?)`.

- [ ] **Step 1: Write failing tests**

In the `tests` module of `src-tauri/src/scripting.rs`, update the helper and add tests:

```rust
    async fn run(script: &str, input: &str) -> ScriptResult {
        let client = spawn_engine(Duration::from_millis(500), Arc::new(|_: &str| None));
        client.run(String::new(), script.to_string(), input.to_string()).await
    }

    #[tokio::test]
    async fn secret_reads_from_resolver_and_missing_is_null() {
        let secrets: SecretFn =
            Arc::new(|name| (name == "TOKEN").then(|| "s3cr3t".to_string()));
        let client = spawn_engine(Duration::from_millis(500), secrets);
        let res = client
            .run(
                String::new(),
                "request.tok = secret('TOKEN'); request.miss = secret('NOPE');".into(),
                r#"{"request":{}}"#.into(),
            )
            .await;
        assert_eq!(res.action, "continue", "err: {:?}", res.error);
        let req = res.request.unwrap();
        assert_eq!(req["tok"], "s3cr3t");
        assert!(req["miss"].is_null());
    }

    #[tokio::test]
    async fn notify_collects_notifications() {
        let res = run(
            "notify('hello', { channel: 'ops', title: 'T' }); notify('plain');",
            r#"{"request":{}}"#,
        )
        .await;
        assert_eq!(res.action, "continue", "err: {:?}", res.error);
        assert_eq!(res.notifications.len(), 2);
        assert_eq!(res.notifications[0]["text"], "hello");
        assert_eq!(res.notifications[0]["channel"], "ops");
        assert_eq!(res.notifications[0]["title"], "T");
        assert_eq!(res.notifications[1]["text"], "plain");
    }

    #[tokio::test]
    async fn handler_supports_secret_and_notify() {
        let secrets: SecretFn = Arc::new(|_| Some("tok".to_string()));
        let res = tokio::task::spawn_blocking(move || {
            execute_handler(
                "",
                "notify('from handler'); return { status: 200, headers: {}, body: secret('X') };",
                r#"{"request":{}}"#,
                Duration::from_secs(5),
                secrets,
            )
        })
        .await
        .unwrap();
        assert_eq!(res.action, "respond", "err: {:?}", res.error);
        assert_eq!(res.response.unwrap()["body"], "tok");
        assert_eq!(res.notifications.len(), 1);
        assert_eq!(res.notifications[0]["text"], "from handler");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test scripting` (from `src-tauri/`)
Expected: compile errors (`spawn_engine` takes 1 arg, no `SecretFn`, no `notifications` field). That is the failure signal for signature-level TDD; proceed.

- [ ] **Step 3: Implement in `scripting.rs`**

3a. Add the type near the top (after imports):

```rust
/// Resolves a named secret for scripts. Injected so tests avoid the real Keychain.
pub type SecretFn = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;
```

3b. Add the field to `ScriptResult` and its `error()` constructor:

```rust
    /// notify(...) calls collected during the run.
    #[serde(default)]
    pub notifications: Vec<serde_json::Value>,
```

(in `error()`: `notifications: Vec::new(),`)

3c. `spawn_engine(timeout: Duration, secrets: SecretFn)` — after `let ctx = Context::full(&rt)...` inside the thread, bind the resolver:

```rust
            ctx.with(|c| {
                let sfn = secrets.clone();
                let f = Function::new(c.clone(), move |name: String| -> Option<String> {
                    sfn(&name)
                })
                .expect("bind secret fn");
                c.globals().set("__native_secret", f).expect("set secret fn");
            });
```

3d. `execute_handler(..., secrets: SecretFn)` — bind the same `__native_secret` next to the existing `__native_send`/`__native_sleep` bindings:

```rust
        let sfn = secrets.clone();
        let secret_fn = match Function::new(c.clone(), move |name: String| -> Option<String> {
            sfn(&name)
        }) {
            Ok(f) => f,
            Err(e) => return ScriptResult::error(format!("bind secret: {e}")),
        };
        let _ = g.set("__native_secret", secret_fn);
```

3e. Append to `STD_LIB` (single-line functions, same style):

```text
function secret(name){var v=__native_secret(String(name));return (v===undefined||v===null)?null:v;}
function notify(text,opts){opts=opts||{};ctx.__notifications.push({text:String(text),channel:opts.channel,title:opts.title});}
```

3f. In `build_source`: after the `ctx.breakpoint = ...` line add `ctx.__notifications = [];` and extend the returned JSON with `notifications: ctx.__notifications`:

```text
      env: ctx.env,
      notifications: ctx.__notifications
```

3g. In `build_handler_source`: after `if (!ctx.env) ctx.env = {};` add `ctx.__notifications = [];`; extend both non-catch returns with `notifications: ctx.__notifications` (the "handler не вернул ответ" error JSON and the `respond` JSON).

- [ ] **Step 4: Fix the two other call sites so the crate compiles**

- `src-tauri/src/commands.rs` `AppState::new()`:

```rust
            scripts: crate::scripting::spawn_engine(
                std::time::Duration::from_secs(1),
                Arc::new(|name: &str| crate::secrets::get(name).ok().flatten()),
            ),
```

- `src-tauri/src/proxy.rs`: the `execute_handler(...)` call (~line 704) gets a temporary extra arg `Arc::new(|_: &str| None)` (replaced by the real resolver in Task 3), and the tests' `scripting(...)` helper's `spawn_engine(...)` call gets `Arc::new(|_: &str| None)` as the second arg. Also update **every other direct `spawn_engine(...)` call in `scripting.rs` tests** (e.g. `infinite_loop_times_out`) the same way.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test` (from `src-tauri/`)
Expected: all pass, including the 3 new tests and all pre-existing scripting/proxy tests.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/scripting.rs src-tauri/src/commands.rs src-tauri/src/proxy.rs
git commit -m "feat(scripting): secret() and notify() in rule scripts"
```

---

### Task 3: Thread notifications and secrets through the proxy

**Files:**
- Modify: `src-tauri/src/proxy.rs`
- Modify: `src-tauri/src/commands.rs` (`start_proxy`)

**Interfaces:**
- Consumes: `ScriptResult.notifications`, `SecretFn` (Task 2).
- Produces: `pub type NotifyFn = Arc<dyn Fn(serde_json::Value) + Send + Sync>` in `proxy.rs`; `proxy::start(addr, store, emit, notify, secret_fn, ca_dir, ...)` (two new params inserted after `emit`); Tauri event **`script-notify`** with payload `{ text, channel?, title?, source: "rule", ruleName, flowId }`.

- [ ] **Step 1: Write the failing integration test**

In `src-tauri/src/proxy.rs` tests, add helpers and a test (mirror the existing proxy test scaffolding around line 1350 — upstream TCP listener, `scripting(...)` helper, request through the proxy via `reqwest` as other tests do):

```rust
    fn notify_noop() -> NotifyFn {
        Arc::new(|_| {})
    }
    fn secret_none() -> crate::scripting::SecretFn {
        Arc::new(|_: &str| None)
    }

    #[tokio::test]
    async fn rule_notify_reaches_notify_fn() {
        // Rule that fires a notification on every request.
        let rule = Rule {
            id: "r1".into(),
            name: "notifier".into(),
            enabled: true,
            projectId: None,
            pattern: "*".into(),
            phase: "request".into(),
            script: "notify('hello', { channel: 'ops' });".into(),
        };
        // NOTE: match the actual Rule struct fields in rules.rs (serde names may
        // be snake_case in Rust); copy construction style from existing tests.
        let (ntx, mut nrx) = tokio::sync::mpsc::unbounded_channel::<serde_json::Value>();
        let notify: NotifyFn = Arc::new(move |p| {
            let _ = ntx.send(p);
        });
        // ...start upstream + proxy exactly like the existing capture test,
        // passing `notify` and `secret_none()` to start(), with vec![rule].
        // ...send one GET through the proxy.
        let p = tokio::time::timeout(std::time::Duration::from_secs(5), nrx.recv())
            .await
            .expect("notification not emitted")
            .unwrap();
        assert_eq!(p["text"], "hello");
        assert_eq!(p["channel"], "ops");
        assert_eq!(p["source"], "rule");
        assert_eq!(p["ruleName"], "notifier");
        assert!(p["flowId"].is_u64());
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test rule_notify_reaches_notify_fn` (from `src-tauri/`)
Expected: compile error — `NotifyFn` and the new `start()` params don't exist yet.

- [ ] **Step 3: Implement threading**

3a. In `proxy.rs` near `EmitFn`:

```rust
/// Delivers a script notification payload to the app (Tauri event `script-notify`).
pub type NotifyFn = Arc<dyn Fn(serde_json::Value) + Send + Sync>;
```

3b. `CaptureHandler` gains fields `notify: NotifyFn` and `secret_fn: crate::scripting::SecretFn`; `start()` gains params `notify: NotifyFn, secret_fn: crate::scripting::SecretFn` inserted **after `emit`**, passed into the handler construction.

3c. Add a helper method on `CaptureHandler`:

```rust
    /// Forward notify() calls collected by a rule script to the app.
    fn emit_notifications(&self, res: &crate::scripting::ScriptResult, rule_name: &str, flow_id: u64) {
        for n in &res.notifications {
            let mut p = serde_json::Map::new();
            p.insert("text".into(), n.get("text").cloned().unwrap_or_else(|| "".into()));
            for k in ["channel", "title"] {
                if let Some(v) = n.get(k).filter(|v| !v.is_null()) {
                    p.insert(k.into(), v.clone());
                }
            }
            p.insert("source".into(), "rule".into());
            p.insert("ruleName".into(), rule_name.into());
            p.insert("flowId".into(), flow_id.into());
            (self.notify)(serde_json::Value::Object(p));
        }
    }
```

3d. Call it at all three script sites, right after the result is obtained:
- request phase (`let res = self.scripts.run(...)` ~line 816): `self.emit_notifications(&res, &rule.name, id);`
- response phase (~line 1031): same call (use that scope's flow id variable).
- handler phase (~line 704): replace the Task 2 placeholder `Arc::new(|_: &str| None)` with `self.secret_fn.clone()` (clone into a local before the `spawn_blocking` closure), then `self.emit_notifications(&res, &hrule.name, id);` after the result.

3e. `commands.rs` `start_proxy`: build and pass the real functions:

```rust
    let app_for_notify = app.clone();
    let notify: proxy::NotifyFn = std::sync::Arc::new(move |payload: serde_json::Value| {
        let _ = app_for_notify.emit("script-notify", payload);
    });
    let secret_fn: crate::scripting::SecretFn =
        std::sync::Arc::new(|name: &str| crate::secrets::get(name).ok().flatten());
```

and pass `notify, secret_fn` to `proxy::start(...)` after `emit`.

3f. Update every existing `start(...)` call in proxy tests: insert `notify_noop(), secret_none(),` after the `emit` argument (~8 call sites).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test` (from `src-tauri/`)
Expected: all pass including `rule_notify_reaches_notify_fn`.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/proxy.rs src-tauri/src/commands.rs
git commit -m "feat(proxy): forward script notify() to the app as script-notify"
```

---

### Task 4: Event registry on the plugin bus

**Files:**
- Modify: `src/plugins/bus.ts`
- Test: `src/plugins/bus.test.ts`

**Interfaces:**
- Produces: `EventMeta { description?: string; payloadType?: string; source?: string }`, `EventInfo extends EventMeta { type: string; lastPayload?: unknown }`; `EventBus.describe(type, meta)`, `EventBus.known(): EventInfo[]`; `emit()` records the last payload per type.

- [ ] **Step 1: Write failing tests** (append to `src/plugins/bus.test.ts`)

```ts
describe("event registry", () => {
  it("known() lists described events with their meta", () => {
    const b = new EventBus();
    b.describe("core:x", { description: "d", payloadType: "{ a: number }", source: "core" });
    expect(b.known()).toEqual([
      {
        type: "core:x",
        description: "d",
        payloadType: "{ a: number }",
        source: "core",
        lastPayload: undefined,
      },
    ]);
  });

  it("emit() records the last payload, and undeclared events appear in known()", () => {
    const b = new EventBus();
    b.emit("p:evt", { n: 1 });
    b.emit("p:evt", { n: 2 });
    expect(b.known()).toEqual([{ type: "p:evt", lastPayload: { n: 2 } }]);
  });

  it("declared meta merges with the observed payload, sorted by type", () => {
    const b = new EventBus();
    b.describe("b:evt", { payloadType: "{ ok: boolean }" });
    b.emit("b:evt", { ok: true });
    b.emit("a:evt", 42);
    expect(b.known().map((e) => e.type)).toEqual(["a:evt", "b:evt"]);
    expect(b.known()[1]).toMatchObject({ payloadType: "{ ok: boolean }", lastPayload: { ok: true } });
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `pnpm vitest run src/plugins/bus.test.ts`
Expected: FAIL — `describe`/`known` are not functions.

- [ ] **Step 3: Implement in `bus.ts`**

```ts
type Handler = (payload: unknown) => void;

export interface EventMeta {
  description?: string;
  /** TS type expression for the payload, e.g. "{ a: number }" — fed to Monaco. */
  payloadType?: string;
  /** Self-reported origin ("core" or a plugin id). */
  source?: string;
}

export interface EventInfo extends EventMeta {
  type: string;
  /** Last payload observed for this event (undefined until first emit). */
  lastPayload?: unknown;
}

/** Minimal typed pub/sub for host↔plugin (and plugin↔plugin) communication. */
export class EventBus {
  private map = new Map<string, Set<Handler>>();
  private meta = new Map<string, EventMeta>();
  private last = new Map<string, unknown>();

  // ...existing on/off unchanged...

  emit(type: string, payload?: unknown): void {
    this.last.set(type, payload);
    this.map.get(type)?.forEach((h) => {
      try {
        h(payload);
      } catch (e) {
        console.error(`[trawl] plugin handler for "${type}" threw`, e);
      }
    });
  }

  /** Declare an event and its payload type (for the subscription UI + hints). */
  describe(type: string, meta: EventMeta): void {
    this.meta.set(type, meta);
  }

  /** Declared and observed events, sorted by type. */
  known(): EventInfo[] {
    const types = new Set([...this.meta.keys(), ...this.last.keys()]);
    return [...types]
      .sort()
      .map((type) => ({ type, ...this.meta.get(type), lastPayload: this.last.get(type) }));
  }
}
```

- [ ] **Step 4: Run tests**

Run: `pnpm vitest run src/plugins/bus.test.ts`
Expected: PASS (old + 3 new tests).

- [ ] **Step 5: Commit**

```bash
git add src/plugins/bus.ts src/plugins/bus.test.ts
git commit -m "feat(plugins): event registry — describe/known + last-payload capture"
```

---

### Task 5: Host API surface — secrets, registry, editor, bridge, stdlib docs

**Files:**
- Create: `src/secrets.ts`
- Modify: `src/plugins/api.ts`, `src/plugins/host.ts`, `src/monaco-setup.ts`, `src/scripting/stdlib.ts`

**Interfaces:**
- Consumes: bus registry (Task 4), Tauri commands (Task 1), `script-notify` event (Task 3), `ScriptEditor` (`src/components/ScriptEditor.tsx`), `analyzeJson`/`fieldsToType` (`src/lib/analyze.ts`).
- Produces (plugin-visible, `HOST_VERSION = "1.6.0"`): `host.secrets.{list,get,set,remove}`; `host.events.{describe,known}`; `host.ui.ScriptEditor`; `host.util.{inferTypeBody(samples), inferFields(samples), setPayloadType(typeBody)}`; bus event `notify:send`; core event descriptions.

- [ ] **Step 1: Frontend secrets wrappers** — create `src/secrets.ts`:

```ts
import { invoke } from "@tauri-apps/api/core";

export const listSecrets = (): Promise<string[]> => invoke("secrets_list");
export const getSecret = (name: string): Promise<string | null> => invoke("secret_get", { name });
export const setSecret = (name: string, value: string): Promise<void> =>
  invoke("secret_set", { name, value });
export const deleteSecret = (name: string): Promise<void> => invoke("secret_delete", { name });
```

- [ ] **Step 2: Extend `src/plugins/api.ts`**

Re-export the bus types and extend the interfaces (existing members unchanged):

```ts
export type { EventInfo, EventMeta } from "./bus";

/** App-wide named secrets (macOS Keychain). Shared with rule scripts (secret()). */
export interface TrawlSecrets {
  list(): Promise<string[]>;
  get(name: string): Promise<string | null>;
  set(name: string, value: string): Promise<void>;
  remove(name: string): Promise<void>;
}

export interface PluginEvents {
  on(type: string, cb: (payload: unknown) => void): () => void;
  off(type: string, cb: (payload: unknown) => void): void;
  emit(type: string, payload?: unknown): void;
  /** Declare an event + payload type so other plugins get hints for it. */
  describe(type: string, meta: EventMeta): void;
  /** Declared and observed events (with last payloads) for subscription UIs. */
  known(): EventInfo[];
}
```

`TrawlUi` gains:

```ts
  /** Monaco-backed code editor wired to the host's completion setup. */
  ScriptEditor: React.ComponentType<{
    value: string;
    onChange: (v: string) => void;
    language?: string;
  }>;
```

`TrawlUtil` gains:

```ts
  /** TS type expression inferred from sample values (for setPayloadType). */
  inferTypeBody(samples: unknown[]): string;
  /** Flat field list (path/type/example) inferred from sample values. */
  inferFields(samples: unknown[]): { path: string; type: string; example?: string }[];
  /** Type the global `payload` in Monaco editors (subscription condition hints). */
  setPayloadType(typeBody: string): void;
```

`TrawlHost` gains `secrets: TrawlSecrets;`.

- [ ] **Step 3: `src/monaco-setup.ts`** — add alongside `setResponseDataType`:

```ts
let payloadDisposable: { dispose: () => void } | null = null;

/** Types the global `payload` for event-subscription editors (plugins). */
export function setEventPayloadType(typeBody: string) {
  payloadDisposable?.dispose();
  payloadDisposable = jsDefaults.addExtraLib(
    `declare const payload: ${typeBody};`,
    "ts:trawl-event-payload.d.ts",
  );
}
```

- [ ] **Step 4: `src/plugins/host.ts`**

- `HOST_VERSION = "1.6.0"`.
- Imports: `ScriptEditor` from `@/components/ScriptEditor`; `analyzeJson, fieldsToType` from `@/lib/analyze`; `setEventPayloadType` from `@/monaco-setup`; secrets wrappers from `@/secrets`.
- `events`: add `describe: (t, m) => bus.describe(t, m)` and `known: () => bus.known()`.
- Add `secrets` object:

```ts
    secrets: {
      list: () => listSecrets(),
      get: (name: string) => getSecret(name),
      set: (name: string, value: string) => setSecret(name, value),
      remove: (name: string) => deleteSecret(name),
    },
```

- `ui`: add `ScriptEditor`.
- `util`: add

```ts
      inferTypeBody: (samples: unknown[]) => fieldsToType(analyzeJson(samples)),
      inferFields: (samples: unknown[]) =>
        analyzeJson(samples).map(({ path, type, example }) => ({ path, type, example })),
      setPayloadType: (typeBody: string) => setEventPayloadType(typeBody),
```

- After the existing `listen("flow-updated", ...)` bridge add:

```ts
  // Script notify() → plugin bus (delivery is a plugin concern, e.g. Telegram).
  void listen("script-notify", (e) => bus.emit("notify:send", e.payload));
```

- Describe core events (same place, after the bridges):

```ts
  const FLOW_TYPE = `{
    id: number; timestamp: number; method: string;
    url: { scheme: string; host: string; port: number; path: string };
    request: { headers: [string, string][]; body: number[] | string; bodyIsText: boolean };
    response: { status: number; headers: [string, string][]; body: number[] | string; bodyIsText: boolean } | null;
    state: string; error: string | null; appliedRules: string[];
  }`;
  bus.describe("flow:added", {
    description: "A new request/response was captured",
    payloadType: FLOW_TYPE,
    source: "core",
  });
  bus.describe("flow:updated", {
    description: "A captured flow changed (response arrived, breakpoint resolved)",
    payloadType: FLOW_TYPE,
    source: "core",
  });
  bus.describe("capture:started", { description: "The proxy started", source: "core" });
  bus.describe("capture:stopped", { description: "The proxy stopped", source: "core" });
  bus.describe("filter:changed", {
    description: "The traffic search/filter changed",
    payloadType: "{ [key: string]: any }",
    source: "core",
  });
  bus.describe("project:changed", {
    description: "The active project selector changed",
    payloadType: "string | null",
    source: "core",
  });
  bus.describe("notify:send", {
    description: "Deliver a notification (emitted by rule notify() and plugins)",
    payloadType:
      "{ text: string; channel?: string; title?: string; source?: string; ruleName?: string; flowId?: number }",
    source: "core",
  });
```

- [ ] **Step 5: `src/scripting/stdlib.ts`** — append to `STD_DTS`:

```ts
/** Read an app-wide named secret (Settings → Secrets, macOS Keychain). Null when missing. */
declare function secret(name: string): string | null;
/**
 * Queue a notification for delivery (e.g. Telegram via the notifications
 * plugin). Emitted to plugins as the "notify:send" bus event after the rule runs.
 */
declare function notify(text: string, opts?: { channel?: string; title?: string }): void;
```

and to `STD_FUNCTIONS`:

```ts
  { signature: "secret(name): string | null", doc: "Read an app-wide named secret (Keychain)." },
  {
    signature: "notify(text, { channel, title })",
    doc: "Queue a notification — delivered by the notifications plugin.",
  },
```

- [ ] **Step 6: Verify**

Run: `pnpm test && pnpm build`
Expected: vitest green, `tsc`/vite build clean.

- [ ] **Step 7: Commit**

```bash
git add src/secrets.ts src/plugins/api.ts src/plugins/host.ts src/monaco-setup.ts src/scripting/stdlib.ts
git commit -m "feat(plugins): host API 1.6.0 — secrets, event registry, notify bridge, payload hints"
```

---

### Task 6: Secrets UI in Setup

**Files:**
- Create: `src/components/SecretsSection.tsx`
- Modify: `src/components/SetupPanel.tsx` (mount at the bottom)

**Interfaces:**
- Consumes: `src/secrets.ts` wrappers (Task 5), `./ui/button`, `./ui/input` (match their actual exported prop APIs).

- [ ] **Step 1: Create the component**

```tsx
import { useEffect, useState } from "react";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { deleteSecret, listSecrets, setSecret } from "@/secrets";

/** App-wide named secrets (stored in the macOS Keychain). */
export function SecretsSection() {
  const [names, setNames] = useState<string[]>([]);
  const [name, setName] = useState("");
  const [value, setValue] = useState("");
  const [error, setError] = useState<string | null>(null);

  const refresh = () =>
    listSecrets()
      .then((n) => {
        setNames(n);
        setError(null);
      })
      .catch((e) => setError(String(e)));
  useEffect(() => {
    void refresh();
  }, []);

  const add = async () => {
    const n = name.trim();
    if (!n || !value) return;
    try {
      await setSecret(n, value);
      setName("");
      setValue("");
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <section className="mt-8">
      <h3 className="mb-1 text-base font-semibold">Secrets</h3>
      <p className="mb-3 text-sm text-muted-foreground">
        App-wide named secrets, stored in the macOS Keychain. Available to rule scripts via{" "}
        <code>secret('NAME')</code> and to plugins. Re-add a name to change its value.
      </p>
      {error && <p className="mb-2 text-sm text-red-500">{error}</p>}
      <ul className="mb-3 space-y-1">
        {names.map((n) => (
          <li
            key={n}
            className="flex items-center justify-between rounded border border-border px-3 py-1.5 text-sm"
          >
            <span className="font-mono">{n}</span>
            <span className="flex items-center gap-2">
              <span className="text-muted-foreground">••••••••</span>
              <Button variant="ghost" size="sm" onClick={() => void deleteSecret(n).then(refresh)}>
                Delete
              </Button>
            </span>
          </li>
        ))}
        {names.length === 0 && <li className="text-sm text-muted-foreground">No secrets yet.</li>}
      </ul>
      <div className="flex gap-2">
        <Input
          placeholder="NAME"
          value={name}
          onChange={(e) => setName(e.target.value)}
          className="w-48 font-mono"
        />
        <Input
          placeholder="value"
          type="password"
          value={value}
          onChange={(e) => setValue(e.target.value)}
        />
        <Button onClick={() => void add()}>Add</Button>
      </div>
    </section>
  );
}
```

Adjust `Button`/`Input` props to the actual exports in `src/components/ui/` if their APIs differ (check before writing).

- [ ] **Step 2: Mount it** — in `SetupPanel.tsx`, import `SecretsSection` and render `<SecretsSection />` as the last child of the top-level scroll container (`<div className="mx-auto h-full max-w-2xl overflow-auto p-6">`).

- [ ] **Step 3: Verify**

Run: `pnpm build`
Expected: clean build. (Manual check happens in Task 15's app run.)

- [ ] **Step 4: Commit**

```bash
git add src/components/SecretsSection.tsx src/components/SetupPanel.tsx
git commit -m "feat(ui): Secrets section in Setup"
```

---

### Task 7: Docs + version bump

**Files:**
- Modify: `docs/plugins.md`, `README.md`, `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`

- [ ] **Step 1: `docs/plugins.md`** — apply these additions:

1. In the `TrawlHost` listing add `secrets: TrawlSecrets;   // app-wide named secrets (Keychain)`.
2. New section after `gitHosts`:

```markdown
### `secrets` — app-wide named secrets

Stored in the macOS Keychain, managed in **Setup → Secrets**. Shared with rule
scripts (`secret('NAME')`).

​```ts
await host.secrets.list();          // string[]
await host.secrets.get("TG_BOT_TOKEN");   // string | null
await host.secrets.set("TG_BOT_TOKEN", token);
await host.secrets.remove("TG_BOT_TOKEN");
​```
```

3. In **Events**, document the registry:

```markdown
### Event registry & payload hints

Declare your events so other plugins can subscribe with autocomplete:

​```ts
host.events.describe("my-plugin:did-thing", {
  description: "Fired after the thing is done",
  payloadType: "{ id: string; ok: boolean }",   // TS type expression
  source: "my-plugin",
});
host.events.known();  // [{ type, description?, payloadType?, source?, lastPayload? }]
​```

The bus also remembers the **last payload** of every event, so undeclared
events still get structure-based hints.
```

4. Add to the host-emitted events table:

```markdown
| `notify:send` | `{ text, channel?, title?, source?, ruleName?, flowId? }` | A rule script called `notify()` (or a plugin asked for a notification). Handled by notification plugins (e.g. Telegram). |
```

5. Mention in the intro bullet list: secrets access and the notifications contract.

- [ ] **Step 2: `README.md`** — in the features list add one line: rule scripts can send notifications (`notify()`) delivered by the notifications plugin, and read Keychain secrets (`secret()`).

- [ ] **Step 3: Version bump to 0.5.0** in `package.json` (`"version"`), `src-tauri/Cargo.toml` (`[package] version`), `src-tauri/tauri.conf.json` (`"version"`).

- [ ] **Step 4: Verify + commit**

Run: `pnpm build && (cd src-tauri && cargo check)`

```bash
git add docs/plugins.md README.md package.json src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json
git commit -m "docs: secrets + event registry + notify contract; bump 0.5.0"
```

---

## Part B — plugin repo (`/Users/legostin/claude-projects/trawl-plugin-notifications`)

### Task 8: Scaffold the plugin repo

**Files (all Create):**
- `trawl-plugin.json`, `package.json`, `tsconfig.json`, `vite.config.ts`, `src/trawl.d.ts`, `src/plugin.tsx`, `.gitignore`

**Interfaces:**
- Produces: buildable IIFE bundle registering an empty "Notifications" mode; `src/trawl.d.ts` `TrawlHost` subset consumed by every later task.

- [ ] **Step 1: Create the repo and files**

```bash
mkdir -p /Users/legostin/claude-projects/trawl-plugin-notifications/src
cd /Users/legostin/claude-projects/trawl-plugin-notifications && git init
```

`trawl-plugin.json`:

```json
{
  "id": "notifications",
  "name": "Notifications",
  "version": "0.1.0",
  "description": "Subscribe to Trawl events and deliver notifications to Telegram.",
  "author": "legostin",
  "entry": "dist/plugin.js",
  "apiVersion": "1.6.0"
}
```

`package.json`:

```json
{
  "name": "trawl-plugin-notifications",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "build": "tsc && vite build",
    "dev": "vite build --watch",
    "test": "vitest run"
  },
  "devDependencies": {
    "@types/react": "^19.1.8",
    "@vitejs/plugin-react": "^4.6.0",
    "react": "^19.1.0",
    "react-dom": "^19.1.0",
    "typescript": "~5.8.3",
    "vite": "^7.0.4",
    "vitest": "^3.2.4"
  }
}
```

`tsconfig.json`: copy verbatim from `/Users/legostin/claude-projects/trawl-plugin-http-client/tsconfig.json`.

`vite.config.ts`:

```ts
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// One IIFE bundle; React + JSX runtime come from the host globals.
export default defineConfig({
  plugins: [react()],
  build: {
    lib: {
      entry: "src/plugin.tsx",
      name: "TrawlNotificationsPlugin",
      formats: ["iife"],
      fileName: () => "plugin.js",
    },
    rollupOptions: {
      external: ["react", "react-dom", "react/jsx-runtime"],
      output: {
        globals: {
          react: "React",
          "react-dom": "ReactDOM",
          "react/jsx-runtime": "ReactJSXRuntime",
        },
      },
    },
    outDir: "dist",
    emptyOutDir: true,
    cssCodeSplit: false,
  },
});
```

`.gitignore`:

```
node_modules
```

`src/trawl.d.ts`:

```ts
// Ambient declaration of the Trawl host API (subset used by this plugin).
// Mirror of the host's src/plugins/api.ts (apiVersion 1.6.0).
import type { ComponentType } from "react";

export interface EventMeta {
  description?: string;
  /** TS type expression for the payload, e.g. "{ a: number }". */
  payloadType?: string;
  source?: string;
}
export interface EventInfo extends EventMeta {
  type: string;
  lastPayload?: unknown;
}

export interface SendRequest {
  method: string;
  url: string;
  headers: [string, string][];
  body: string;
}
export interface SendResponse {
  status: number;
  headers: [string, string][];
  body: string;
  bodyIsText: boolean;
  durationMs: number;
  error: string | null;
}

export interface FieldInfo {
  path: string;
  type: string;
  example?: string;
}

export interface TrawlHost {
  version: string;
  react: typeof import("react");
  events: {
    on(type: string, cb: (payload: unknown) => void): () => void;
    off(type: string, cb: (payload: unknown) => void): void;
    emit(type: string, payload?: unknown): void;
    describe(type: string, meta: EventMeta): void;
    known(): EventInfo[];
  };
  http: { send(req: SendRequest, viaProxy?: boolean): Promise<SendResponse> };
  secrets: {
    list(): Promise<string[]>;
    get(name: string): Promise<string | null>;
    set(name: string, value: string): Promise<void>;
    remove(name: string): Promise<void>;
  };
  storage: {
    get(key: string): Promise<string | null>;
    set(key: string, value: string): Promise<void>;
  };
  ui: {
    ScriptEditor: ComponentType<{
      value: string;
      onChange: (v: string) => void;
      language?: string;
    }>;
  };
  util: {
    inferTypeBody(samples: unknown[]): string;
    inferFields(samples: unknown[]): FieldInfo[];
    setPayloadType(typeBody: string): void;
  };
  registerMode(mode: {
    id: string;
    label: string;
    icon?: ComponentType<{ className?: string }>;
    component: ComponentType;
  }): void;
  setMode(id: string): void;
  log(...args: unknown[]): void;
}

declare global {
  interface Window {
    __TRAWL__?: TrawlHost;
  }
}
```

`src/plugin.tsx` (stub — replaced in Task 12):

```tsx
const host = window.__TRAWL__;

function Panel() {
  return <div style={{ padding: 16 }}>Notifications — coming up.</div>;
}

if (host) {
  host.registerMode({ id: "notifications", label: "Notifications", component: Panel });
}
```

- [ ] **Step 2: Install + build**

Run: `pnpm install && pnpm build`
Expected: `dist/plugin.js` emitted, tsc clean.

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "scaffold: notifications plugin (manifest, vite IIFE build, host types)"
```

---

### Task 9: Model + template/condition/throttle logic

**Files:**
- Create: `src/model.ts`, `src/render.ts`
- Test: `src/render.test.ts`

**Interfaces:**
- Produces: `Channel { name: string; type: "telegram"; tokenSecretName: string; chatId: string }`, `Subscription { id: string; event: string; channel: string; template: string; condition?: string; throttleSec?: number; enabled: boolean }`, `LogEntry { ts: number; event: string; channel: string; text: string; ok: boolean; error?: string }`, `Config { channels: Channel[]; subscriptions: Subscription[] }`; `renderTemplate(tpl: string, payload: unknown): string`, `evalCondition(cond: string | undefined, payload: unknown): boolean`, `throttled(lastTs: number | undefined, throttleSec: number | undefined, now: number): boolean`.

- [ ] **Step 1: `src/model.ts`**

```ts
export interface Channel {
  name: string;
  type: "telegram";
  /** Name of the core secret holding the bot token. */
  tokenSecretName: string;
  chatId: string;
}

export interface Subscription {
  id: string;
  event: string;
  /** Channel name; empty string = first configured channel. */
  channel: string;
  /** Message text with {{payload.expr}} placeholders (JS expressions). */
  template: string;
  /** Optional JS expression; falsy result → skip. */
  condition?: string;
  /** Min seconds between sends for this subscription. */
  throttleSec?: number;
  enabled: boolean;
}

export interface LogEntry {
  ts: number;
  event: string;
  channel: string;
  text: string;
  ok: boolean;
  error?: string;
}

export interface Config {
  channels: Channel[];
  subscriptions: Subscription[];
}
```

- [ ] **Step 2: Failing tests** — `src/render.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { evalCondition, renderTemplate, throttled } from "./render";

describe("renderTemplate", () => {
  it("evaluates {{}} as JS expressions against payload", () => {
    const out = renderTemplate("{{payload.method}} {{payload.url.path}} took {{payload.ms + 1}}ms", {
      method: "GET",
      url: { path: "/x" },
      ms: 41,
    });
    expect(out).toBe("GET /x took 42ms");
  });

  it("stringifies objects and blanks null/undefined", () => {
    expect(renderTemplate("{{payload.o}}|{{payload.missing}}", { o: { a: 1 } })).toBe('{"a":1}|');
  });

  it("renders {{error}} for a broken expression", () => {
    expect(renderTemplate("x {{payload..}} y", {})).toBe("x {{error}} y");
  });
});

describe("evalCondition", () => {
  it("empty condition is true", () => {
    expect(evalCondition(undefined, {})).toBe(true);
    expect(evalCondition("   ", {})).toBe(true);
  });
  it("evaluates against payload", () => {
    expect(evalCondition("payload.status >= 500", { status: 502 })).toBe(true);
    expect(evalCondition("payload.status >= 500", { status: 200 })).toBe(false);
  });
  it("a throwing condition is false", () => {
    expect(evalCondition("payload.a.b.c", {})).toBe(false);
  });
});

describe("throttled", () => {
  it("no throttle or no previous send → not throttled", () => {
    expect(throttled(undefined, 60, 1000)).toBe(false);
    expect(throttled(500, undefined, 1000)).toBe(false);
    expect(throttled(500, 0, 1000)).toBe(false);
  });
  it("suppresses within the window, allows after", () => {
    expect(throttled(1000, 60, 1000 + 59_000)).toBe(true);
    expect(throttled(1000, 60, 1000 + 60_000)).toBe(false);
  });
});
```

- [ ] **Step 3: Run to verify failure**

Run: `pnpm test`
Expected: FAIL — `./render` does not exist.

- [ ] **Step 4: Implement `src/render.ts`**

```ts
/** Render {{expr}} placeholders; each expr is JS evaluated with `payload` in scope. */
export function renderTemplate(tpl: string, payload: unknown): string {
  return tpl.replace(/\{\{([\s\S]+?)\}\}/g, (_, expr: string) => {
    try {
      const v = new Function("payload", `return (${expr});`)(payload);
      if (v === undefined || v === null) return "";
      return typeof v === "object" ? JSON.stringify(v) : String(v);
    } catch {
      return "{{error}}";
    }
  });
}

/** Empty/blank condition passes; errors fail closed (no notification). */
export function evalCondition(cond: string | undefined, payload: unknown): boolean {
  if (!cond || !cond.trim()) return true;
  try {
    return Boolean(new Function("payload", `return (${cond});`)(payload));
  } catch {
    return false;
  }
}

/** True when a send happened less than throttleSec ago. */
export function throttled(
  lastTs: number | undefined,
  throttleSec: number | undefined,
  now: number,
): boolean {
  if (!throttleSec || lastTs === undefined) return false;
  return now - lastTs < throttleSec * 1000;
}
```

- [ ] **Step 5: Run tests → PASS, then commit**

Run: `pnpm test`

```bash
git add src/model.ts src/render.ts src/render.test.ts
git commit -m "feat: config model + template/condition/throttle logic"
```

---

### Task 10: Telegram sender

**Files:**
- Create: `src/telegram.ts`
- Test: `src/telegram.test.ts`

**Interfaces:**
- Consumes: `Channel` (Task 9), `TrawlHost` (`secrets.get`, `http.send`).
- Produces: `sendTelegram(host: TrawlHost, channel: Channel, text: string): Promise<{ ok: boolean; error?: string }>`.

- [ ] **Step 1: Failing tests** — `src/telegram.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { sendTelegram } from "./telegram";
import type { Channel } from "./model";
import type { SendRequest, TrawlHost } from "./trawl";

const channel: Channel = { name: "ops", type: "telegram", tokenSecretName: "TG", chatId: "42" };

function fakeHost(opts: { token?: string | null; status?: number; body?: string }) {
  const sent: SendRequest[] = [];
  const host = {
    secrets: { get: async () => opts.token ?? null },
    http: {
      send: async (req: SendRequest) => {
        sent.push(req);
        return {
          status: opts.status ?? 200,
          headers: [],
          body: opts.body ?? '{"ok":true}',
          bodyIsText: true,
          durationMs: 1,
          error: null,
        };
      },
    },
  } as unknown as TrawlHost;
  return { host, sent };
}

describe("sendTelegram", () => {
  it("posts sendMessage with token, chat_id and text", async () => {
    const { host, sent } = fakeHost({ token: "12345:abc" });
    const res = await sendTelegram(host, channel, "hello");
    expect(res.ok).toBe(true);
    expect(sent[0].method).toBe("POST");
    expect(sent[0].url).toBe("https://api.telegram.org/bot12345:abc/sendMessage");
    expect(JSON.parse(sent[0].body)).toEqual({ chat_id: "42", text: "hello" });
  });

  it("fails when the secret is missing", async () => {
    const { host, sent } = fakeHost({ token: null });
    const res = await sendTelegram(host, channel, "hello");
    expect(res.ok).toBe(false);
    expect(res.error).toContain('secret "TG"');
    expect(sent).toHaveLength(0);
  });

  it("fails on a non-200 Telegram response", async () => {
    const { host } = fakeHost({ token: "t", status: 403, body: "forbidden" });
    const res = await sendTelegram(host, channel, "hello");
    expect(res.ok).toBe(false);
    expect(res.error).toContain("403");
  });
});
```

- [ ] **Step 2: Run to verify failure** — `pnpm test` → FAIL (`./telegram` missing).

- [ ] **Step 3: Implement `src/telegram.ts`**

```ts
import type { Channel } from "./model";
import type { TrawlHost } from "./trawl";

/** Send `text` to a Telegram channel via the Bot API (token from core secrets). */
export async function sendTelegram(
  host: TrawlHost,
  channel: Channel,
  text: string,
): Promise<{ ok: boolean; error?: string }> {
  const token = await host.secrets.get(channel.tokenSecretName);
  if (!token) return { ok: false, error: `secret "${channel.tokenSecretName}" not found` };
  try {
    const res = await host.http.send(
      {
        method: "POST",
        url: `https://api.telegram.org/bot${token}/sendMessage`,
        headers: [["content-type", "application/json"]],
        body: JSON.stringify({ chat_id: channel.chatId, text }),
      },
      false,
    );
    if (res.status !== 200) {
      return { ok: false, error: `telegram: HTTP ${res.status} ${res.body}`.trim() };
    }
    return { ok: true };
  } catch (e) {
    return { ok: false, error: String(e) };
  }
}
```

- [ ] **Step 4: Run tests → PASS, then commit**

```bash
git add src/telegram.ts src/telegram.test.ts
git commit -m "feat: telegram sender via host.http + core secret"
```

---

### Task 11: Delivery engine

**Files:**
- Create: `src/engine.ts`
- Test: `src/engine.test.ts`

**Interfaces:**
- Consumes: Tasks 9–10 types/functions; `TrawlHost.events.on`, `TrawlHost.storage`.
- Produces: `class Engine` — `constructor(host, send = sendTelegram, now = () => Date.now())`; `config: Config`; `log: LogEntry[]`; `onLog: (() => void) | null`; `load(): Promise<void>`; `saveConfig(c: Config): Promise<void>`; `start(): void`; `stop(): void`; `testChannel(ch: Channel): Promise<{ ok: boolean; error?: string }>`. Storage keys `notifications:config`, `notifications:log`; log cap 200.

- [ ] **Step 1: Failing tests** — `src/engine.test.ts`:

```ts
import { describe, expect, it, vi } from "vitest";
import { Engine } from "./engine";
import type { Config } from "./model";
import type { TrawlHost } from "./trawl";

function fakeHost() {
  const handlers = new Map<string, Set<(p: unknown) => void>>();
  const store = new Map<string, string>();
  const host = {
    events: {
      on(type: string, cb: (p: unknown) => void) {
        if (!handlers.has(type)) handlers.set(type, new Set());
        handlers.get(type)!.add(cb);
        return () => handlers.get(type)!.delete(cb);
      },
      emit(type: string, p?: unknown) {
        handlers.get(type)?.forEach((h) => h(p));
      },
      off() {},
      describe() {},
      known: () => [],
    },
    storage: {
      get: async (k: string) => store.get(k) ?? null,
      set: async (k: string, v: string) => void store.set(k, v),
    },
    log: () => {},
  } as unknown as TrawlHost;
  return { host, emit: (t: string, p?: unknown) => host.events.emit(t, p), store };
}

const config: Config = {
  channels: [
    { name: "ops", type: "telegram", tokenSecretName: "TG", chatId: "1" },
    { name: "alerts", type: "telegram", tokenSecretName: "TG", chatId: "2" },
  ],
  subscriptions: [
    {
      id: "s1",
      event: "flow:added",
      channel: "ops",
      template: "{{payload.method}} {{payload.status}}",
      condition: "payload.status >= 500",
      throttleSec: 60,
      enabled: true,
    },
  ],
};

async function flush() {
  await new Promise((r) => setTimeout(r, 0));
}

describe("Engine", () => {
  it("delivers a matching event through the subscription channel", async () => {
    const { host, emit } = fakeHost();
    const send = vi.fn(async () => ({ ok: true }));
    const e = new Engine(host, send, () => 1000);
    await e.saveConfig(config);
    emit("flow:added", { method: "GET", status: 502 });
    await flush();
    expect(send).toHaveBeenCalledTimes(1);
    expect(send.mock.calls[0][1].name).toBe("ops");
    expect(send.mock.calls[0][2]).toBe("GET 502");
    expect(e.log[0]).toMatchObject({ event: "flow:added", channel: "ops", ok: true });
  });

  it("skips when the condition is false and throttles repeats", async () => {
    const { host, emit } = fakeHost();
    const send = vi.fn(async () => ({ ok: true }));
    let now = 1000;
    const e = new Engine(host, send, () => now);
    await e.saveConfig(config);
    emit("flow:added", { method: "GET", status: 200 }); // condition false
    emit("flow:added", { method: "GET", status: 500 }); // sends
    emit("flow:added", { method: "GET", status: 500 }); // throttled
    now += 61_000;
    emit("flow:added", { method: "GET", status: 500 }); // window passed
    await flush();
    expect(send).toHaveBeenCalledTimes(2);
  });

  it("notify:send routes to the named channel, falls back to the first", async () => {
    const { host, emit } = fakeHost();
    const send = vi.fn(async () => ({ ok: true }));
    const e = new Engine(host, send, () => 1000);
    await e.saveConfig({ ...config, subscriptions: [] });
    emit("notify:send", { text: "to alerts", channel: "alerts" });
    emit("notify:send", { text: "to default", title: "T" });
    await flush();
    expect(send).toHaveBeenCalledTimes(2);
    expect(send.mock.calls[0][1].name).toBe("alerts");
    expect(send.mock.calls[1][1].name).toBe("ops");
    expect(send.mock.calls[1][2]).toBe("T\nto default");
  });

  it("logs a failure when no channel is configured", async () => {
    const { host, emit } = fakeHost();
    const send = vi.fn(async () => ({ ok: true }));
    const e = new Engine(host, send, () => 1000);
    await e.saveConfig({ channels: [], subscriptions: [] });
    emit("notify:send", { text: "nowhere" });
    await flush();
    expect(send).not.toHaveBeenCalled();
    expect(e.log[0]).toMatchObject({ ok: false });
  });

  it("persists config and log, reloads them", async () => {
    const { host, emit, store } = fakeHost();
    const send = vi.fn(async () => ({ ok: true }));
    const e = new Engine(host, send, () => 1000);
    await e.saveConfig(config);
    emit("flow:added", { method: "GET", status: 500 });
    await flush();
    const e2 = new Engine(host, send, () => 1000);
    await e2.load();
    expect(e2.config.channels).toHaveLength(2);
    expect(e2.log).toHaveLength(1);
    expect(store.has("notifications:config")).toBe(true);
  });
});
```

- [ ] **Step 2: Run to verify failure** — `pnpm test` → FAIL (`./engine` missing).

- [ ] **Step 3: Implement `src/engine.ts`**

```ts
import type { Channel, Config, LogEntry, Subscription } from "./model";
import { evalCondition, renderTemplate, throttled } from "./render";
import { sendTelegram } from "./telegram";
import type { TrawlHost } from "./trawl";

const CONFIG_KEY = "notifications:config";
const LOG_KEY = "notifications:log";
const LOG_CAP = 200;

export type Sender = (
  host: TrawlHost,
  channel: Channel,
  text: string,
) => Promise<{ ok: boolean; error?: string }>;

/** Subscribes to bus events per config and delivers rendered messages. */
export class Engine {
  config: Config = { channels: [], subscriptions: [] };
  log: LogEntry[] = [];
  /** UI refresh hook, called after every log append. */
  onLog: (() => void) | null = null;

  private offs: Array<() => void> = [];
  private lastSent = new Map<string, number>();

  constructor(
    private host: TrawlHost,
    private send: Sender = sendTelegram,
    private now: () => number = () => Date.now(),
  ) {}

  async load(): Promise<void> {
    const raw = await this.host.storage.get(CONFIG_KEY);
    if (raw) this.config = JSON.parse(raw) as Config;
    const log = await this.host.storage.get(LOG_KEY);
    if (log) this.log = JSON.parse(log) as LogEntry[];
    this.start();
  }

  async saveConfig(c: Config): Promise<void> {
    this.config = c;
    await this.host.storage.set(CONFIG_KEY, JSON.stringify(c));
    this.start();
  }

  start(): void {
    this.stop();
    for (const sub of this.config.subscriptions.filter((s) => s.enabled)) {
      this.offs.push(
        this.host.events.on(sub.event, (payload) => void this.handleSub(sub, payload)),
      );
    }
    this.offs.push(this.host.events.on("notify:send", (payload) => void this.handleNotify(payload)));
  }

  stop(): void {
    for (const off of this.offs) off();
    this.offs = [];
  }

  /** Channel by name; empty/undefined → first configured. */
  channel(name?: string): Channel | undefined {
    if (name) return this.config.channels.find((c) => c.name === name);
    return this.config.channels[0];
  }

  async testChannel(ch: Channel): Promise<{ ok: boolean; error?: string }> {
    const res = await this.send(this.host, ch, "Trawl: test notification");
    await this.push({
      ts: this.now(),
      event: "(test)",
      channel: ch.name,
      text: "Trawl: test notification",
      ok: res.ok,
      error: res.error,
    });
    return res;
  }

  private async handleSub(sub: Subscription, payload: unknown): Promise<void> {
    if (!evalCondition(sub.condition, payload)) return;
    if (throttled(this.lastSent.get(sub.id), sub.throttleSec, this.now())) return;
    const text = renderTemplate(sub.template, payload);
    await this.deliver(sub.event, sub.channel || undefined, text);
    this.lastSent.set(sub.id, this.now());
  }

  private async handleNotify(payload: unknown): Promise<void> {
    const p = (payload ?? {}) as { text?: string; channel?: string; title?: string };
    const text = p.title ? `${p.title}\n${p.text ?? ""}` : (p.text ?? "");
    await this.deliver("notify:send", p.channel, text);
  }

  private async deliver(event: string, channelName: string | undefined, text: string) {
    const ch = this.channel(channelName);
    if (!ch) {
      await this.push({
        ts: this.now(),
        event,
        channel: channelName ?? "(default)",
        text,
        ok: false,
        error: "no matching channel configured",
      });
      return;
    }
    const res = await this.send(this.host, ch, text);
    await this.push({ ts: this.now(), event, channel: ch.name, text, ok: res.ok, error: res.error });
  }

  private async push(e: LogEntry): Promise<void> {
    this.log = [e, ...this.log].slice(0, LOG_CAP);
    await this.host.storage.set(LOG_KEY, JSON.stringify(this.log));
    this.onLog?.();
  }
}
```

- [ ] **Step 4: Run tests → PASS, then commit**

```bash
git add src/engine.ts src/engine.test.ts
git commit -m "feat: delivery engine — subscriptions, notify:send, throttle, log"
```

---

### Task 12: UI — NotificationsApp + wiring

**Files:**
- Create: `src/NotificationsApp.tsx`
- Modify: `src/plugin.tsx`

**Interfaces:**
- Consumes: `Engine` (Task 11), `host.events.known()`, `host.ui.ScriptEditor`, `host.util.{setPayloadType,inferTypeBody,inferFields}`, `host.secrets.list()`.

- [ ] **Step 1: Create `src/NotificationsApp.tsx`**

Use `host.react` hooks (shared React), the host's Tailwind classes, and no local state libraries. Complete component:

```tsx
import type { Engine } from "./engine";
import type { Channel, Config, LogEntry, Subscription } from "./model";
import type { EventInfo, TrawlHost } from "./trawl";

const host = window.__TRAWL__ as TrawlHost;
const { useEffect, useMemo, useState } = host.react;

export function NotificationsApp({ engine }: { engine: Engine }) {
  const [tab, setTab] = useState<"subs" | "channels" | "log">("subs");
  const [config, setConfig] = useState<Config>(engine.config);
  const [, bump] = useState(0);

  useEffect(() => {
    setConfig(engine.config);
    engine.onLog = () => bump((n) => n + 1);
    return () => {
      engine.onLog = null;
    };
  }, []);

  const save = (c: Config) => {
    setConfig(c);
    void engine.saveConfig(c);
  };

  const tabs = [
    ["subs", "Subscriptions"],
    ["channels", "Channels"],
    ["log", "Log"],
  ] as const;

  return (
    <div className="flex h-full flex-col overflow-auto p-4">
      <div className="mb-3 flex items-center gap-3">
        <h2 className="text-lg font-semibold">Notifications</h2>
        <div className="ml-auto flex gap-1 text-sm">
          {tabs.map(([id, label]) => (
            <button
              key={id}
              onClick={() => setTab(id)}
              className={`rounded px-2 py-1 ${tab === id ? "bg-accent" : "text-muted-foreground hover:text-foreground"}`}
            >
              {label}
            </button>
          ))}
        </div>
      </div>
      {tab === "channels" && <Channels config={config} save={save} engine={engine} />}
      {tab === "subs" && <Subscriptions config={config} save={save} />}
      {tab === "log" && <Log entries={engine.log} />}
    </div>
  );
}

function Channels({
  config,
  save,
  engine,
}: {
  config: Config;
  save: (c: Config) => void;
  engine: Engine;
}) {
  const [secretNames, setSecretNames] = useState<string[]>([]);
  const [draft, setDraft] = useState<Channel>({
    name: "",
    type: "telegram",
    tokenSecretName: "",
    chatId: "",
  });
  const [testResult, setTestResult] = useState<string | null>(null);
  useEffect(() => {
    void host.secrets.list().then(setSecretNames);
  }, []);

  const add = () => {
    if (!draft.name.trim() || !draft.tokenSecretName || !draft.chatId.trim()) return;
    save({ ...config, channels: [...config.channels, { ...draft, name: draft.name.trim() }] });
    setDraft({ name: "", type: "telegram", tokenSecretName: "", chatId: "" });
  };
  const remove = (name: string) =>
    save({ ...config, channels: config.channels.filter((c) => c.name !== name) });
  const test = async (ch: Channel) => {
    setTestResult("sending…");
    const r = await engine.testChannel(ch);
    setTestResult(r.ok ? `✓ sent via ${ch.name}` : `✗ ${r.error}`);
  };

  const input = "rounded border border-border bg-transparent px-2 py-1 text-sm";
  return (
    <div className="max-w-2xl space-y-3">
      <p className="text-sm text-muted-foreground">
        Telegram channels. The bot token lives in the app secrets (Setup → Secrets); a channel
        references it by name.
      </p>
      {config.channels.map((ch) => (
        <div
          key={ch.name}
          className="flex items-center gap-3 rounded border border-border px-3 py-2 text-sm"
        >
          <span className="font-medium">{ch.name}</span>
          <span className="text-muted-foreground">
            telegram · token: {ch.tokenSecretName} · chat: {ch.chatId}
          </span>
          <span className="ml-auto flex gap-2">
            <button className="text-primary" onClick={() => void test(ch)}>
              Send test
            </button>
            <button className="text-destructive" onClick={() => remove(ch.name)}>
              Delete
            </button>
          </span>
        </div>
      ))}
      {config.channels.length === 0 && (
        <p className="text-sm text-muted-foreground">No channels yet.</p>
      )}
      <div className="flex flex-wrap items-center gap-2">
        <input
          className={input}
          placeholder="name (e.g. ops)"
          value={draft.name}
          onChange={(e) => setDraft({ ...draft, name: e.target.value })}
        />
        <select
          className={input}
          value={draft.tokenSecretName}
          onChange={(e) => setDraft({ ...draft, tokenSecretName: e.target.value })}
        >
          <option value="">bot token secret…</option>
          {secretNames.map((n) => (
            <option key={n} value={n}>
              {n}
            </option>
          ))}
        </select>
        <input
          className={input}
          placeholder="chat_id"
          value={draft.chatId}
          onChange={(e) => setDraft({ ...draft, chatId: e.target.value })}
        />
        <button className="rounded bg-primary px-3 py-1 text-sm text-primary-foreground" onClick={add}>
          Add channel
        </button>
      </div>
      {testResult && <p className="text-sm text-muted-foreground">{testResult}</p>}
    </div>
  );
}

function payloadTypeFor(info: EventInfo | undefined): string {
  if (!info) return "any";
  if (info.payloadType) return info.payloadType;
  if (info.lastPayload !== undefined) return host.util.inferTypeBody([info.lastPayload]);
  return "any";
}

function Subscriptions({ config, save }: { config: Config; save: (c: Config) => void }) {
  const [editing, setEditing] = useState<Subscription | null>(null);
  const known = useMemo(() => host.events.known(), [editing]);

  const upsert = (s: Subscription) => {
    const rest = config.subscriptions.filter((x) => x.id !== s.id);
    save({ ...config, subscriptions: [...rest, s] });
    setEditing(null);
  };
  const remove = (id: string) =>
    save({ ...config, subscriptions: config.subscriptions.filter((s) => s.id !== id) });
  const toggle = (s: Subscription) => upsert({ ...s, enabled: !s.enabled });

  if (editing) {
    return (
      <SubscriptionEditor
        sub={editing}
        known={known}
        channels={config.channels}
        onSave={upsert}
        onCancel={() => setEditing(null)}
      />
    );
  }
  return (
    <div className="max-w-3xl space-y-2">
      {config.subscriptions.map((s) => (
        <div key={s.id} className="flex items-center gap-3 rounded border border-border px-3 py-2 text-sm">
          <input type="checkbox" checked={s.enabled} onChange={() => toggle(s)} />
          <span className="font-mono">{s.event}</span>
          <span className="text-muted-foreground">→ {s.channel || "(first channel)"}</span>
          <span className="ml-auto flex gap-2">
            <button className="text-primary" onClick={() => setEditing(s)}>
              Edit
            </button>
            <button className="text-destructive" onClick={() => remove(s.id)}>
              Delete
            </button>
          </span>
        </div>
      ))}
      {config.subscriptions.length === 0 && (
        <p className="text-sm text-muted-foreground">
          No subscriptions. Subscribe to any core or plugin event and get a Telegram message.
        </p>
      )}
      <button
        className="rounded bg-primary px-3 py-1 text-sm text-primary-foreground"
        onClick={() =>
          setEditing({
            id: crypto.randomUUID(),
            event: "flow:added",
            channel: "",
            template: "",
            condition: "",
            throttleSec: 0,
            enabled: true,
          })
        }
      >
        Add subscription
      </button>
    </div>
  );
}

function SubscriptionEditor({
  sub,
  known,
  channels,
  onSave,
  onCancel,
}: {
  sub: Subscription;
  known: EventInfo[];
  channels: Channel[];
  onSave: (s: Subscription) => void;
  onCancel: () => void;
}) {
  const [s, setS] = useState<Subscription>(sub);
  const info = known.find((e) => e.type === s.event);

  // Feed Monaco the payload type for the selected event (declared or inferred).
  useEffect(() => {
    host.util.setPayloadType(payloadTypeFor(info));
  }, [s.event]);

  const fields =
    info?.lastPayload !== undefined ? host.util.inferFields([info.lastPayload]) : [];
  const input = "rounded border border-border bg-transparent px-2 py-1 text-sm";
  const { ScriptEditor } = host.ui;

  return (
    <div className="max-w-3xl space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <select
          className={input}
          value={known.some((e) => e.type === s.event) ? s.event : ""}
          onChange={(e) => e.target.value && setS({ ...s, event: e.target.value })}
        >
          <option value="">custom event…</option>
          {known.map((e) => (
            <option key={e.type} value={e.type} title={e.description}>
              {e.type}
            </option>
          ))}
        </select>
        <input
          className={`${input} font-mono`}
          placeholder="event name"
          value={s.event}
          onChange={(e) => setS({ ...s, event: e.target.value })}
        />
        <select
          className={input}
          value={s.channel}
          onChange={(e) => setS({ ...s, channel: e.target.value })}
        >
          <option value="">(first channel)</option>
          {channels.map((c) => (
            <option key={c.name} value={c.name}>
              {c.name}
            </option>
          ))}
        </select>
        <label className="flex items-center gap-1 text-sm text-muted-foreground">
          throttle, s
          <input
            className={`${input} w-16`}
            type="number"
            min={0}
            value={s.throttleSec ?? 0}
            onChange={(e) => setS({ ...s, throttleSec: Number(e.target.value) || 0 })}
          />
        </label>
      </div>
      {info?.description && <p className="text-sm text-muted-foreground">{info.description}</p>}

      <div>
        <div className="mb-1 text-sm font-medium">Message template</div>
        <textarea
          className="h-24 w-full rounded border border-border bg-transparent p-2 font-mono text-sm"
          placeholder={"{{payload.method}} {{payload.url.path}} → {{payload.response.status}}"}
          value={s.template}
          onChange={(e) => setS({ ...s, template: e.target.value })}
        />
        {fields.length > 0 && (
          <div className="mt-1 flex flex-wrap gap-1">
            {fields.slice(0, 20).map((f) => (
              <button
                key={f.path}
                title={`${f.type}${f.example ? ` · e.g. ${f.example}` : ""}`}
                className="rounded border border-border px-1.5 py-0.5 font-mono text-xs text-muted-foreground hover:text-foreground"
                onClick={() =>
                  setS({
                    ...s,
                    template: `${s.template}{{payload.${f.path.replaceAll("[]", "[0]")}}}`,
                  })
                }
              >
                {f.path}
              </button>
            ))}
          </div>
        )}
      </div>

      <div>
        <div className="mb-1 text-sm font-medium">
          Condition <span className="text-muted-foreground">(JS, optional — e.g. payload.response.status &gt;= 500)</span>
        </div>
        <div className="h-24 overflow-hidden rounded border border-border">
          <ScriptEditor
            value={s.condition ?? ""}
            onChange={(v) => setS({ ...s, condition: v })}
            language="javascript"
          />
        </div>
      </div>

      <div className="flex gap-2">
        <button
          className="rounded bg-primary px-3 py-1 text-sm text-primary-foreground"
          onClick={() => s.event.trim() && s.template.trim() && onSave(s)}
        >
          Save
        </button>
        <button className="rounded border border-border px-3 py-1 text-sm" onClick={onCancel}>
          Cancel
        </button>
      </div>
    </div>
  );
}

function Log({ entries }: { entries: LogEntry[] }) {
  if (entries.length === 0)
    return <p className="text-sm text-muted-foreground">Nothing sent yet.</p>;
  return (
    <div className="max-w-3xl space-y-1">
      {entries.map((e, i) => (
        <div key={i} className="flex items-baseline gap-2 rounded border border-border px-3 py-1.5 text-sm">
          <span className={e.ok ? "text-green-500" : "text-destructive"}>{e.ok ? "✓" : "✗"}</span>
          <span className="text-muted-foreground">{new Date(e.ts).toLocaleTimeString()}</span>
          <span className="font-mono">{e.event}</span>
          <span className="text-muted-foreground">→ {e.channel}</span>
          <span className="truncate">{e.error ?? e.text}</span>
        </div>
      ))}
    </div>
  );
}
```

- [ ] **Step 2: Wire up `src/plugin.tsx`** (replace the stub):

```tsx
import { Engine } from "./engine";
import { NotificationsApp } from "./NotificationsApp";

const host = window.__TRAWL__;

function BellIcon({ className }: { className?: string }) {
  return (
    <svg
      className={className}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9" />
      <path d="M10.3 21a1.94 1.94 0 0 0 3.4 0" />
    </svg>
  );
}

if (host) {
  const engine = new Engine(host);
  void engine.load();
  host.registerMode({
    id: "notifications",
    label: "Notifications",
    icon: BellIcon,
    component: () => <NotificationsApp engine={engine} />,
  });
}
```

- [ ] **Step 3: Verify**

Run: `pnpm test && pnpm build`
Expected: tests green, `dist/plugin.js` builds.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat: notifications UI — channels, subscriptions with payload hints, log"
```

---

### Task 13: Plugin README + built bundle commit

**Files:**
- Create: `README.md`
- Commit: `dist/plugin.js` (Trawl fetches the entry file from the repo)

- [ ] **Step 1: README.md**

```markdown
# trawl-plugin-notifications

Telegram notifications for [Trawl](https://github.com/legostin/http-catch).

- Subscribe to any core or plugin event on the bus; template + optional JS
  condition + throttle per subscription, with payload autocomplete.
- Rule scripts can call `notify("text", { channel })` — delivered here via the
  `notify:send` event.
- Bot tokens live in Trawl's Keychain secrets (Setup → Secrets); channels
  reference a secret by name.

## Setup

1. In Trawl: **Setup → Secrets** — add e.g. `TG_BOT_TOKEN` with your bot token
   (create a bot via @BotFather).
2. **Notifications → Channels** — add a channel: name, token secret, `chat_id`
   (message @userinfobot or use a group chat id). Press **Send test**.
3. **Notifications → Subscriptions** — pick an event, write a template like
   `{{payload.method}} {{payload.url.path}} → {{payload.response.status}}`,
   optionally a condition (`payload.response.status >= 500`) and a throttle.

## Build

​```sh
pnpm install
pnpm build   # emits dist/plugin.js (commit it)
​```

Requires Trawl host API ≥ 1.6.0.
```

- [ ] **Step 2: Build and commit everything including `dist/`**

```bash
pnpm build
git add -A && git commit -m "docs: README; build dist bundle"
```

(Publishing to GitHub `legostin/trawl-plugin-notifications` and installing via the Plugins tab is a user step — ask before pushing.)

---

## Part C — finish

### Task 14: Core verification + merge

- [ ] **Step 1: Full test sweep in the worktree**

Run: `(cd src-tauri && cargo test) && pnpm test && pnpm build`
Expected: everything green.

- [ ] **Step 2: Smoke-run the app** (optional but recommended): `pnpm tauri dev` — check Setup → Secrets add/delete works against the real Keychain, and that a rule with `notify('hi')` writes a `notify:send` event (visible via the notifications plugin log once installed, or `host.events.on("notify:send", console.log)` in devtools).

- [ ] **Step 3: Merge to main** per the worktree→merge workflow (superpowers:finishing-a-development-branch): merge the worktree branch into `main`, delete the worktree.

- [ ] **Step 4: Update `docs/plugins.md` reference** — none beyond Task 7; confirm the spec's checkboxes are all covered.
