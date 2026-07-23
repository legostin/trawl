# Git Host Tokens in Keychain + Settings UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Store git-host tokens in the macOS Keychain (migrating the plaintext `git-hosts.json` map) and add a "Git hosts" section to the Settings panel so a github.com token can be set from the UI.

**Architecture:** Backend keeps the existing `host_token`/`save_git_host_token` signatures in `src-tauri/src/plugins.rs` but backs them with Keychain entries named `git-host:<host>` under the existing `trawl` service; `git-hosts.json` becomes a host-name array (Keychain can't enumerate). A new `git_hosts_list` command feeds a new `GitHostsSection` React component in the Settings panel, mirroring `SecretsSection`.

**Tech Stack:** Rust (tauri, keyring, serde), React + TypeScript (zustand not needed), vitest, cargo test.

**Spec:** `docs/superpowers/specs/2026-07-23-git-host-tokens-keychain-design.md`

## Global Constraints

- Public Rust signatures unchanged: `host_token(data_dir: &Path, host: &str) -> Option<String>`, `save_git_host_token(data_dir: &Path, host: &str, token: &str) -> Result<()>`; empty token = delete.
- Tauri commands `git_host_token_set/has/get` keep names and signatures.
- Keychain account name: `git-host:<host>` (e.g. `git-host:github.com`), service `trawl`.
- Git-host tokens must NOT appear in the `secrets.json` index or the Secrets UI list.
- All Rust tests: `cd src-tauri && cargo test --lib`. All frontend tests: `pnpm test`. Frontend typecheck+build: `pnpm build`.
- Commit trailer on every commit:
  `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>` and
  `Claude-Session: https://claude.ai/code/session_019wS92NjgewRWFd5AaQ3REU`

---

### Task 1: Shared mock Keychain test module

Move the in-memory Keychain mock out of `secrets::tests` so `plugins.rs` tests can use it too. Pure refactor — existing tests keep passing.

**Files:**
- Modify: `src-tauri/src/secrets.rs` (the `#[cfg(test)] mod tests` block at the bottom)

**Interfaces:**
- Produces: `crate::secrets::testutil::mock_store()` — installs the in-memory credential builder once (idempotent, `Once`-guarded). `#[cfg(test)]` only.

- [ ] **Step 1: Move the mock into a `testutil` module**

In `src-tauri/src/secrets.rs`, cut `TestCredential`, `TestCredentialBuilder`, and `mock_store` out of `mod tests` and paste them into a new module directly above it:

```rust
/// In-memory Keychain mock shared by secrets and plugins tests.
#[cfg(test)]
pub mod testutil {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use keyring::credential::{CredentialApi, CredentialBuilderApi};

    // …TestCredential and TestCredentialBuilder move here unchanged…

    /// Install the mock credential builder (idempotent).
    pub fn mock_store() {
        use std::sync::Once;
        static ONCE: Once = Once::new();
        ONCE.call_once(|| {
            let store = std::sync::Arc::new(Mutex::new(HashMap::new()));
            let builder = TestCredentialBuilder { store };
            keyring::set_default_credential_builder(Box::new(builder));
        });
    }
}
```

Inside the module, `TestCredential`/`TestCredentialBuilder` keep their bodies verbatim; make both structs and the `store` fields `pub(crate)`-visible only as needed (module-private structs + `pub fn mock_store()` is enough). In `mod tests`, replace the old definitions with `use super::testutil::mock_store;`.

- [ ] **Step 2: Run the Rust suite to verify the refactor is clean**

Run: `cd src-tauri && cargo test --lib secrets::`
Expected: all 4 secrets tests PASS, no warnings about unused items.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/secrets.rs
git commit -m "refactor(secrets): extract shared mock Keychain test module"
```

---

### Task 2: Unindexed Keychain helpers in secrets.rs

`set`/`delete` maintain the `secrets.json` name index; git-host tokens need Keychain writes that bypass it.

**Files:**
- Modify: `src-tauri/src/secrets.rs` (below `delete`, above `data_dir`)
- Test: `src-tauri/src/secrets.rs` (`mod tests`)

**Interfaces:**
- Consumes: `testutil::mock_store()` from Task 1.
- Produces: `pub fn set_unindexed(name: &str, value: &str) -> Result<()>` and `pub fn delete_unindexed(name: &str) -> Result<()>` in `crate::secrets`. `get(name)` already reads by name without the index — Task 3 uses all three.

- [ ] **Step 1: Write the failing test**

Append to `secrets.rs` `mod tests`:

```rust
#[test]
fn unindexed_set_get_delete_skips_index() {
    mock_store();
    let dir = tmp_dir("unindexed");
    set_unindexed("git-host:example.test", "tok-1").unwrap();
    assert_eq!(get("git-host:example.test").unwrap().as_deref(), Some("tok-1"));
    // Not in the secrets.json index.
    assert!(list_names(&dir).is_empty());
    delete_unindexed("git-host:example.test").unwrap();
    assert_eq!(get("git-host:example.test").unwrap(), None);
    // Deleting a missing entry is not an error.
    delete_unindexed("git-host:example.test").unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --lib secrets::`
Expected: compile error — `set_unindexed` not found (Rust's RED for a missing symbol).

- [ ] **Step 3: Write minimal implementation**

Add to `secrets.rs` after `delete`:

```rust
/// Keychain write that skips the secrets.json index — for values with their
/// own index and UI (git-host tokens), which must not show up in Secrets.
pub fn set_unindexed(name: &str, value: &str) -> Result<()> {
    Entry::new(SERVICE, name)?.set_password(value)?;
    Ok(())
}

pub fn delete_unindexed(name: &str) -> Result<()> {
    match Entry::new(SERVICE, name)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test --lib secrets::`
Expected: 5 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/secrets.rs
git commit -m "feat(secrets): unindexed Keychain set/delete for git-host tokens"
```

---

### Task 3: Keychain-backed git-host storage with migration

Replace the plaintext token map in `plugins.rs` with Keychain entries + a host-name array index, migrating old files on first read.

**Files:**
- Modify: `src-tauri/src/plugins.rs` — the `── Per-host git tokens (git-hosts.json) ──` section (functions `git_hosts_path`, `load_git_hosts`, `host_token`, `save_git_host_token`)
- Test: `src-tauri/src/plugins.rs` (`mod tests`)

**Interfaces:**
- Consumes: `crate::secrets::{get, set_unindexed, delete_unindexed}` (Task 2), `crate::secrets::testutil::mock_store` (Task 1).
- Produces: `pub fn list_git_hosts(data_dir: &Path) -> Vec<String>` (sorted host names; runs migration). `host_token` / `save_git_host_token` signatures unchanged. `load_git_hosts` is deleted.

- [ ] **Step 1: Write the failing tests**

Append to `plugins.rs` `mod tests` (note: tests share one process-wide mock store, so each test uses unique host names):

```rust
fn tmp_data_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("trawl-githosts-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn git_host_token_roundtrip_in_keychain() {
    crate::secrets::testutil::mock_store();
    let dir = tmp_data_dir("roundtrip");
    save_git_host_token(&dir, "gh-rt.test", "tok-abc").unwrap();
    assert_eq!(host_token(&dir, "gh-rt.test").as_deref(), Some("tok-abc"));
    assert_eq!(list_git_hosts(&dir), vec!["gh-rt.test".to_string()]);
    // The index file holds host names only — never token material.
    let text = std::fs::read_to_string(dir.join("git-hosts.json")).unwrap();
    assert!(!text.contains("tok-abc"));
    // Empty token deletes entry and index row.
    save_git_host_token(&dir, "gh-rt.test", "").unwrap();
    assert_eq!(host_token(&dir, "gh-rt.test"), None);
    assert!(list_git_hosts(&dir).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn old_plaintext_map_migrates_to_keychain() {
    crate::secrets::testutil::mock_store();
    let dir = tmp_data_dir("migrate");
    std::fs::write(
        dir.join("git-hosts.json"),
        r#"{ "gh-mig.test": "tok-old", "ghe-mig.test": "tok-ghe" }"#,
    )
    .unwrap();
    // First read migrates: tokens resolve, file is rewritten as a name array.
    assert_eq!(host_token(&dir, "gh-mig.test").as_deref(), Some("tok-old"));
    assert_eq!(
        list_git_hosts(&dir),
        vec!["gh-mig.test".to_string(), "ghe-mig.test".to_string()]
    );
    let text = std::fs::read_to_string(dir.join("git-hosts.json")).unwrap();
    assert!(!text.contains("tok-old") && !text.contains("tok-ghe"));
    serde_json::from_str::<Vec<String>>(&text).expect("index is a plain array now");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn missing_or_corrupt_git_hosts_file_is_empty() {
    crate::secrets::testutil::mock_store();
    let dir = tmp_data_dir("corrupt");
    assert!(list_git_hosts(&dir).is_empty());
    assert_eq!(host_token(&dir, "gh-none.test"), None);
    std::fs::write(dir.join("git-hosts.json"), "not json").unwrap();
    assert!(list_git_hosts(&dir).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test --lib plugins::`
Expected: compile error — `list_git_hosts` not found.

- [ ] **Step 3: Replace the storage implementation**

In `plugins.rs`, replace `load_git_hosts`, `host_token`, and `save_git_host_token` (keep `git_hosts_path`) with:

```rust
fn git_host_secret(host: &str) -> String {
    format!("git-host:{host}")
}

/// Hosts with a stored token, sorted. Migrates the pre-0.9.1 plaintext
/// `{host: token}` map into the Keychain on first read; the file keeps only
/// host names afterwards (the Keychain cannot enumerate its entries).
pub fn list_git_hosts(data_dir: &Path) -> Vec<String> {
    let Ok(text) = fs::read_to_string(git_hosts_path(data_dir)) else {
        return Vec::new();
    };
    if let Ok(hosts) = serde_json::from_str::<Vec<String>>(&text) {
        return hosts;
    }
    let Ok(map) = serde_json::from_str::<HashMap<String, String>>(&text) else {
        return Vec::new();
    };
    let mut hosts: Vec<String> = Vec::new();
    for (host, token) in &map {
        if crate::secrets::set_unindexed(&git_host_secret(host), token).is_ok() {
            hosts.push(host.clone());
        }
    }
    hosts.sort();
    if let Ok(json) = serde_json::to_string_pretty(&hosts) {
        let _ = fs::write(git_hosts_path(data_dir), json);
    }
    hosts
}

pub fn host_token(data_dir: &Path, host: &str) -> Option<String> {
    let _ = list_git_hosts(data_dir); // ensure a pre-0.9.1 file has migrated
    crate::secrets::get(&git_host_secret(host)).ok().flatten()
}

/// An empty token removes the host's entry.
pub fn save_git_host_token(data_dir: &Path, host: &str, token: &str) -> Result<()> {
    fs::create_dir_all(data_dir).context("create data dir")?;
    let mut hosts = list_git_hosts(data_dir);
    let token = token.trim();
    if token.is_empty() {
        crate::secrets::delete_unindexed(&git_host_secret(host))?;
        hosts.retain(|h| h != host);
    } else {
        crate::secrets::set_unindexed(&git_host_secret(host), token)?;
        if !hosts.iter().any(|h| h == host) {
            hosts.push(host.to_string());
            hosts.sort();
        }
    }
    fs::write(git_hosts_path(data_dir), serde_json::to_string_pretty(&hosts)?)
        .context("write git-hosts.json")?;
    Ok(())
}
```

`load_git_hosts` had no callers besides the two functions above — delete it. Update the section header comment to `// ── Per-host git tokens (Keychain; git-hosts.json holds host names only) ──`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test --lib`
Expected: full suite PASS (141 tests: 138 + 3 new), no `dead_code` warning for removed `load_git_hosts`.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/plugins.rs
git commit -m "feat(plugins): git host tokens live in the Keychain; migrate plaintext git-hosts.json"
```

---

### Task 4: `git_hosts_list` command

**Files:**
- Modify: `src-tauri/src/plugins.rs` (next to `git_host_token_set`, ~line 494)
- Modify: `src-tauri/src/lib.rs` (the `generate_handler![...]` list, after `plugins::git_host_token_set` at line 91)

**Interfaces:**
- Consumes: `list_git_hosts` (Task 3).
- Produces: Tauri command `git_hosts_list` → `Vec<String>`; the frontend invokes it as `invoke("git_hosts_list")` (Task 5).

- [ ] **Step 1: Add the command**

In `plugins.rs`, below `git_host_token_get`:

```rust
#[tauri::command]
pub fn git_hosts_list(app: AppHandle) -> Result<Vec<String>, String> {
    Ok(list_git_hosts(&data_dir(&app)?))
}
```

In `lib.rs`, add `plugins::git_hosts_list,` right after the `plugins::git_host_token_set,` line.

- [ ] **Step 2: Verify it compiles and the suite stays green**

Run: `cd src-tauri && cargo test --lib`
Expected: PASS. (The command body is a one-line wrapper over the Task 3 function, which is already covered; no new test.)

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/plugins.rs src-tauri/src/lib.rs
git commit -m "feat(plugins): git_hosts_list command"
```

---

### Task 5: Frontend git-hosts API + host normalization

**Files:**
- Create: `src/gitHosts.ts`
- Test: `src/gitHosts.test.ts`

**Interfaces:**
- Consumes: Tauri commands `git_hosts_list`, `git_host_token_set`.
- Produces: `normalizeHost(input: string): string`; `listGitHosts(): Promise<string[]>`; `setGitHostToken(host, token): Promise<void>`; `deleteGitHostToken(host): Promise<void>`. Task 6 imports all four from `@/gitHosts`.

- [ ] **Step 1: Write the failing test**

`src/gitHosts.test.ts`:

```typescript
import { describe, it, expect, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { normalizeHost } from "./gitHosts";

describe("normalizeHost", () => {
  it("defaults empty input to github.com", () => {
    expect(normalizeHost("")).toBe("github.com");
    expect(normalizeHost("   ")).toBe("github.com");
  });

  it("strips scheme, www and any path", () => {
    expect(normalizeHost("https://github.example.org")).toBe("github.example.org");
    expect(normalizeHost("http://www.github.com/owner/repo")).toBe("github.com");
    expect(normalizeHost("github.com/")).toBe("github.com");
  });

  it("keeps a bare host as-is", () => {
    expect(normalizeHost("github.example.org")).toBe("github.example.org");
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm test`
Expected: FAIL — `./gitHosts` module not found.

- [ ] **Step 3: Write the implementation**

`src/gitHosts.ts`:

```typescript
import { invoke } from "@tauri-apps/api/core";

/** User host input → bare hostname; empty means github.com. */
export const normalizeHost = (input: string): string => {
  const bare = input.trim().replace(/^https?:\/\//, "").replace(/^www\./, "");
  return bare.split("/")[0] || "github.com";
};

export const listGitHosts = (): Promise<string[]> => invoke("git_hosts_list");
export const setGitHostToken = (host: string, token: string): Promise<void> =>
  invoke("git_host_token_set", { host, token });
export const deleteGitHostToken = (host: string): Promise<void> =>
  invoke("git_host_token_set", { host, token: "" });
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm test`
Expected: 20 files / 88 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/gitHosts.ts src/gitHosts.test.ts
git commit -m "feat(ui): git-hosts frontend API with host normalization"
```

---

### Task 6: GitHostsSection in Settings

**Files:**
- Create: `src/components/GitHostsSection.tsx`
- Modify: `src/components/SettingsPanel.tsx`

**Interfaces:**
- Consumes: `@/gitHosts` (Task 5); `./ui/button`, `./ui/input` (existing).
- Produces: `GitHostsSection` React component rendered by `SettingsPanel`.

- [ ] **Step 1: Write the component**

`src/components/GitHostsSection.tsx` (mirrors `SecretsSection.tsx` structure exactly):

```tsx
import { useEffect, useState } from "react";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { deleteGitHostToken, listGitHosts, normalizeHost, setGitHostToken } from "@/gitHosts";

/** Access tokens for git hosts plugins are fetched from (macOS Keychain). */
export function GitHostsSection() {
  const [hosts, setHosts] = useState<string[]>([]);
  const [host, setHost] = useState("");
  const [token, setToken] = useState("");
  const [error, setError] = useState<string | null>(null);

  const refresh = () =>
    listGitHosts()
      .then((h) => {
        setHosts(h);
        setError(null);
      })
      .catch((e) => setError(String(e)));
  useEffect(() => {
    void refresh();
  }, []);

  const add = async () => {
    if (!token.trim()) return;
    try {
      await setGitHostToken(normalizeHost(host), token);
      setHost("");
      setToken("");
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <section>
      <h3 className="mb-1 text-base font-semibold">Git hosts</h3>
      <p className="mb-3 text-sm text-muted-foreground">
        Tokens for installing and updating plugins, stored in the macOS Keychain. A{" "}
        <code>github.com</code> token raises the API rate limit that anonymous update checks
        can hit (HTTP 403). Re-add a host to change its token.
      </p>
      {error && <p className="mb-2 text-sm text-red-500">{error}</p>}
      <ul className="mb-3 space-y-1">
        {hosts.map((h) => (
          <li
            key={h}
            className="flex items-center justify-between rounded border border-border px-3 py-1.5 text-sm"
          >
            <span className="font-mono">{h}</span>
            <span className="flex items-center gap-2">
              <span className="text-muted-foreground">••••••••</span>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => void deleteGitHostToken(h).then(refresh).catch((e) => setError(String(e)))}
              >
                Delete
              </Button>
            </span>
          </li>
        ))}
        {hosts.length === 0 && <li className="text-sm text-muted-foreground">No tokens yet.</li>}
      </ul>
      <div className="flex gap-2">
        <Input
          placeholder="github.com"
          value={host}
          onChange={(e) => setHost(e.target.value)}
          className="w-48 font-mono"
        />
        <Input
          placeholder="token"
          type="password"
          value={token}
          onChange={(e) => setToken(e.target.value)}
        />
        <Button onClick={() => void add()}>Add</Button>
      </div>
    </section>
  );
}
```

- [ ] **Step 2: Wire into SettingsPanel**

`src/components/SettingsPanel.tsx` — add the import and render between Secrets and MCP, and widen the description:

```tsx
import { SecretsSection } from "./SecretsSection";
import { GitHostsSection } from "./GitHostsSection";
import { McpSection } from "./McpSection";

/** App configuration (not part of the connection guide): secrets, git host
 *  tokens, and the MCP server. Each section owns its own persistence; the
 *  panel only lays them out with a consistent vertical rhythm. */
export function SettingsPanel() {
  return (
    <div className="mx-auto h-full max-w-2xl overflow-auto p-6">
      <h2 className="mb-1 text-lg font-semibold">Settings</h2>
      <p className="mb-6 text-sm text-muted-foreground">
        App-wide configuration: secrets, git host tokens, and the MCP server for AI agents.
      </p>

      <div className="space-y-8">
        <SecretsSection />
        <GitHostsSection />
        <McpSection />
      </div>
    </div>
  );
}
```

- [ ] **Step 3: Typecheck, build, and run both suites**

Run: `pnpm build && pnpm test && cd src-tauri && cargo test --lib`
Expected: tsc + vite build clean; 88 frontend tests PASS; 141 Rust tests PASS.

- [ ] **Step 4: Commit**

```bash
git add src/components/GitHostsSection.tsx src/components/SettingsPanel.tsx
git commit -m "feat(ui): Git hosts section in Settings"
```
