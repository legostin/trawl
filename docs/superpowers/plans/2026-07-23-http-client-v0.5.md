# HTTP Client v0.5.0 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add MCP tools (full client control), collection folders + rename, cURL import/export, and a proxy switch with host-side auto-start to the HTTP Client plugin.

**Architecture:** Almost all work happens in the plugin repo `~/claude-projects/trawl-plugin-http-client` (React IIFE bundle running inside the Trawl host). One small change in the host repo `~/claude-projects/http-catch`: auto-start the proxy when a plugin sends `viaProxy`. Collections move to a v2 stored format with nested folders; MCP tools are registered at plugin init and work headless via storage + `H.http.send`, with module-level pub/sub keeping an open UI in sync.

**Tech Stack:** TypeScript, React 19 (inline styles, no UI lib in plugin), vitest, zustand (host only), Tauri host API via `window.__TRAWL__`.

**Spec:** `docs/superpowers/specs/2026-07-23-http-client-v0.4-design.md` (target version is now **0.5.0** — the repo already shipped 0.4.0 with request tabs / raw sub-types).

## Global Constraints

- Plugin work happens on a branch in an **isolated worktree** of `~/claude-projects/trawl-plugin-http-client` (user's standard workflow: worktree → merge to main). Same for the host change in `~/claude-projects/http-catch`.
- Base commit (plugin): `869ee87`. Base commit (host): `b720868` or newer main.
- Plugin storage keys: collection `httpclient.collection.<projectId|_global>` (unchanged), new `httpclient.viaProxy` (global).
- Collection stored format v2: `{ version: 2, folders: [...], items: [...] }`; v1 (flat array) migrates on read, never written back as v1.
- MCP tool names are registered as `<name>` and exposed by the host as `http-client_<name>`.
- Response bodies over **200 000 chars** are truncated with `truncated: true`.
- Plugin version → **0.5.0** in both `package.json` and `trawl-plugin.json`; manifest `apiVersion` → **"1.5.0"** (first host API with `mcp.registerTool`; current host is 1.7.0).
- No `window.prompt`/`window.confirm`/`alert` — they don't work reliably in the Tauri webview (see commit 1701361). Use inline inputs and two-click delete confirmation.
- Tests: `pnpm test` (vitest) in each repo. Build: `pnpm build`.
- Code style: match existing — inline `styles` objects, 2-space indent, `void` for fire-and-forget promises, comments only for non-obvious constraints.

---

### Task 1: Host — auto-start proxy on `viaProxy` sends

**Files:**
- Modify: `~/claude-projects/http-catch/src/store.ts` (add `ensureProxy` action, ~line 50 interface + ~line 101 impl)
- Modify: `~/claude-projects/http-catch/src/plugins/host.ts:174-176` (`http.send` wrapper)
- Test: `~/claude-projects/http-catch/src/store.test.ts` (append describe block)

**Interfaces:**
- Consumes: existing `startProxy(port)` store action, `sendRequest` from `@/http`.
- Produces: `useFlows.getState().ensureProxy(): Promise<void>` — starts the proxy on 8729 and sets `{running: true, proxyAddr}` iff not already running. Plugin-facing behavior: `H.http.send(req, true)` never fails with "connection refused" because the proxy is down.

- [ ] **Step 1: Write the failing test** — append to `src/store.test.ts`:

```ts
describe("flows store — ensureProxy", () => {
  beforeEach(() => {
    invoke.mockReset();
    useFlows.setState({ running: false, proxyAddr: null });
  });

  it("starts the proxy and marks it running when stopped", async () => {
    invoke.mockResolvedValue("0.0.0.0:8729");
    await useFlows.getState().ensureProxy();
    expect(invoke).toHaveBeenCalledWith("start_proxy", { port: 8729 });
    expect(useFlows.getState().running).toBe(true);
    expect(useFlows.getState().proxyAddr).toBe("0.0.0.0:8729");
  });

  it("does nothing when the proxy is already running", async () => {
    useFlows.setState({ running: true, proxyAddr: "0.0.0.0:8729" });
    await useFlows.getState().ensureProxy();
    expect(invoke).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd ~/claude-projects/http-catch && pnpm test -- src/store.test.ts`
Expected: FAIL — `ensureProxy is not a function`.

- [ ] **Step 3: Implement.** In `src/store.ts`, add to the `FlowsState` interface (next to `toggleProxy`):

```ts
  /** Start the proxy if it isn't running (used by plugin sends with viaProxy). */
  ensureProxy: () => Promise<void>;
```

and to the store implementation (next to `toggleProxy`):

```ts
  ensureProxy: async () => {
    const { running, startProxy } = get();
    if (running) return;
    const addr = await startProxy(8729);
    set({ running: true, proxyAddr: addr });
  },
```

In `src/plugins/host.ts` replace the `http` block:

```ts
    http: {
      // viaProxy must work even when the proxy is stopped: start it on demand
      // so the request is captured and the topbar reflects the running proxy.
      send: async (req, viaProxy) => {
        if (viaProxy) await useFlows.getState().ensureProxy();
        return sendRequest(req, viaProxy);
      },
    },
```

- [ ] **Step 4: Run tests**

Run: `cd ~/claude-projects/http-catch && pnpm test`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add src/store.ts src/plugins/host.ts src/store.test.ts
git commit -m "feat(plugins): auto-start proxy for viaProxy sends (ensureProxy)"
```

---

### Task 2: Plugin — collections v2: folders, rename, move, recursive delete, change events

**Files:**
- Rewrite: `~/claude-projects/trawl-plugin-http-client/src/collections.ts`
- Rewrite test: `~/claude-projects/trawl-plugin-http-client/src/collections.test.ts`

**Interfaces:**
- Consumes: `ClientRequest` from `./model`, `H.storage`, `H.projects.active()`.
- Produces (all used by Tasks 5, 6, 8):

```ts
export interface SavedRequest { id: string; name: string; folderId: string | null; request: ClientRequest }
export interface Folder { id: string; name: string; parentId: string | null }
export interface Collection { version: 2; folders: Folder[]; items: SavedRequest[] }
export function emptyCollection(): Collection
export function onCollectionChange(cb: (c: Collection) => void): () => void
export async function loadCollection(): Promise<Collection>            // migrates v1 arrays
export async function saveCollection(c: Collection): Promise<void>     // persists + notifies
export async function addToCollection(name: string, request: ClientRequest, folderId?: string | null): Promise<Collection>
export async function updateInCollection(id: string, name: string, request: ClientRequest): Promise<Collection>
export async function createFolder(name: string, parentId?: string | null): Promise<Collection>
export async function renameItem(id: string, name: string): Promise<Collection>          // request or folder
export async function moveItem(id: string, targetFolderId: string | null): Promise<Collection> // request→folderId, folder→parentId; throws on cycles
export async function deleteItem(id: string): Promise<Collection>      // folder = recursive
export function folderDescendants(c: Collection, folderId: string): Set<string>  // includes folderId itself
export function folderPath(c: Collection, folderId: string | null): string       // "a / b / c", "" for root
```

- [ ] **Step 1: Write the failing tests.** Replace `src/collections.test.ts` (keep the existing `window.__TRAWL__` stub header and the four `updateInCollection` tests, adapted to the new return shape — `next.items[...]` instead of `next[...]`):

```ts
import { describe, it, expect, beforeEach } from "vitest";
import { blankRequest } from "./model";

// collections.ts reads window.__TRAWL__ at module load — stub it before import.
const store = new Map<string, string>();
(globalThis as { window?: unknown }).window = {
  __TRAWL__: {
    projects: { active: () => ({ id: "p1", name: "P", env: [] }) },
    storage: {
      get: async (k: string) => store.get(k) ?? null,
      set: async (k: string, v: string) => void store.set(k, v),
    },
  },
};

const {
  addToCollection, updateInCollection, loadCollection, createFolder,
  renameItem, moveItem, deleteItem, folderDescendants, folderPath, onCollectionChange,
} = await import("./collections");

const KEY = "httpclient.collection.p1";

describe("v1 → v2 migration", () => {
  beforeEach(() => store.clear());

  it("reads a v1 flat array as a v2 collection with folderId null", async () => {
    store.set(KEY, JSON.stringify([{ id: "a", name: "old", request: blankRequest() }]));
    const c = await loadCollection();
    expect(c.version).toBe(2);
    expect(c.folders).toEqual([]);
    expect(c.items).toHaveLength(1);
    expect(c.items[0].folderId).toBeNull();
    expect(c.items[0].name).toBe("old");
  });

  it("returns an empty v2 collection for missing or corrupt data", async () => {
    expect((await loadCollection()).items).toEqual([]);
    store.set(KEY, "not json");
    expect((await loadCollection()).version).toBe(2);
  });

  it("persists as v2 after the first write", async () => {
    store.set(KEY, JSON.stringify([{ id: "a", name: "old", request: blankRequest() }]));
    await addToCollection("new", blankRequest());
    const raw = JSON.parse(store.get(KEY)!);
    expect(raw.version).toBe(2);
    expect(raw.items).toHaveLength(2);
  });
});

describe("folders", () => {
  beforeEach(() => store.clear());

  it("creates nested folders and computes paths", async () => {
    const c1 = await createFolder("api");
    const api = c1.folders[0];
    const c2 = await createFolder("users", api.id);
    const users = c2.folders[1];
    expect(users.parentId).toBe(api.id);
    expect(folderPath(c2, users.id)).toBe("api / users");
    expect(folderPath(c2, null)).toBe("");
  });

  it("saves a request into a folder", async () => {
    const c1 = await createFolder("api");
    const c2 = await addToCollection("r", blankRequest(), c1.folders[0].id);
    expect(c2.items[0].folderId).toBe(c1.folders[0].id);
  });

  it("renames folders and requests", async () => {
    const cf = await createFolder("api");
    const cr = await addToCollection("r", blankRequest());
    let c = await renameItem(cf.folders[0].id, "api2");
    expect(c.folders[0].name).toBe("api2");
    c = await renameItem(cr.items[0].id, "r2");
    expect(c.items[0].name).toBe("r2");
  });

  it("moves a request between folders and to the root", async () => {
    const cf = await createFolder("api");
    const fid = cf.folders[0].id;
    const cr = await addToCollection("r", blankRequest());
    let c = await moveItem(cr.items[0].id, fid);
    expect(c.items[0].folderId).toBe(fid);
    c = await moveItem(cr.items[0].id, null);
    expect(c.items[0].folderId).toBeNull();
  });

  it("moves a folder under another folder", async () => {
    const c1 = await createFolder("a");
    const c2 = await createFolder("b");
    const [a, b] = [c1.folders[0].id, c2.folders[1].id];
    const c = await moveItem(b, a);
    expect(c.folders.find((f) => f.id === b)!.parentId).toBe(a);
  });

  it("rejects moving a folder into itself or a descendant", async () => {
    const c1 = await createFolder("a");
    const a = c1.folders[0].id;
    const c2 = await createFolder("b", a);
    const b = c2.folders[1].id;
    await expect(moveItem(a, a)).rejects.toThrow(/itself|descendant/);
    await expect(moveItem(a, b)).rejects.toThrow(/itself|descendant/);
  });

  it("folderDescendants includes the folder and all nested folders", async () => {
    const c1 = await createFolder("a");
    const a = c1.folders[0].id;
    const c2 = await createFolder("b", a);
    const b = c2.folders[1].id;
    const c3 = await createFolder("c", b);
    const set = folderDescendants(c3, a);
    expect(set.has(a)).toBe(true);
    expect(set.has(b)).toBe(true);
    expect(set.has(c3.folders[2].id)).toBe(true);
  });

  it("deletes a folder recursively with its subfolders and requests", async () => {
    const c1 = await createFolder("a");
    const a = c1.folders[0].id;
    const c2 = await createFolder("b", a);
    const b = c2.folders[1].id;
    await addToCollection("in-a", blankRequest(), a);
    await addToCollection("in-b", blankRequest(), b);
    await addToCollection("root", blankRequest());
    const c = await deleteItem(a);
    expect(c.folders).toEqual([]);
    expect(c.items.map((i) => i.name)).toEqual(["root"]);
  });

  it("deletes a single request by id", async () => {
    const c1 = await addToCollection("r", blankRequest());
    const c = await deleteItem(c1.items[0].id);
    expect(c.items).toEqual([]);
  });

  it("notifies subscribers after every write", async () => {
    const seen: number[] = [];
    const off = onCollectionChange((c) => seen.push(c.items.length));
    await addToCollection("r", blankRequest());
    off();
    await addToCollection("r2", blankRequest());
    expect(seen).toEqual([1]);
  });
});

describe("updateInCollection", () => {
  beforeEach(() => store.clear());

  it("updates an existing entry in place, keeping its name, id and folder", async () => {
    const cf = await createFolder("api");
    const added = await addToCollection("first", { ...blankRequest(), url: "https://a" }, cf.folders[0].id);
    const id = added.items[0].id;
    const next = await updateInCollection(id, "first", { ...blankRequest(), url: "https://b" });
    expect(next.items).toHaveLength(1);
    expect(next.items[0].id).toBe(id);
    expect(next.items[0].folderId).toBe(cf.folders[0].id);
    expect(next.items[0].request.url).toBe("https://b");
  });

  it("appends the request when the entry was deleted meanwhile", async () => {
    const next = await updateInCollection("gone", "orphan", { ...blankRequest(), url: "https://c" });
    expect(next.items).toHaveLength(1);
    expect(next.items[0].name).toBe("orphan");
    expect(next.items[0].folderId).toBeNull();
  });

  it("strips file bytes like addToCollection does", async () => {
    const added = await addToCollection("f", blankRequest());
    const req = {
      ...blankRequest(),
      multipartFiles: [{ key: "f", enabled: true, fileName: "a.bin", fileB64: "AAAA", contentType: "x" }],
    };
    const next = await updateInCollection(added.items[0].id, "f", req);
    expect(next.items[0].request.multipartFiles[0].fileB64).toBe("");
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd <plugin worktree> && pnpm test -- src/collections.test.ts`
Expected: FAIL — `createFolder` etc. not exported; shape mismatches.

- [ ] **Step 3: Rewrite `src/collections.ts`:**

```ts
import type { ClientRequest } from "./model";

const H = window.__TRAWL__!;

export interface SavedRequest {
  id: string;
  name: string;
  folderId: string | null;
  request: ClientRequest;
}

export interface Folder {
  id: string;
  name: string;
  parentId: string | null;
}

/** Stored format v2. v1 (a flat SavedRequest[] without folderId) migrates on read. */
export interface Collection {
  version: 2;
  folders: Folder[];
  items: SavedRequest[];
}

export const emptyCollection = (): Collection => ({ version: 2, folders: [], items: [] });

/** Storage key for the active project's collection ("_global" when none). */
function key(): string {
  const p = H.projects.active();
  return `httpclient.collection.${p ? p.id : "_global"}`;
}

/** Strip heavy file bytes before persisting (files are re-picked on load). */
function lighten(req: ClientRequest): ClientRequest {
  return {
    ...req,
    multipartFiles: req.multipartFiles.map((f) => ({ ...f, fileB64: "" })),
  };
}

function migrate(parsed: unknown): Collection {
  if (Array.isArray(parsed)) {
    return {
      version: 2,
      folders: [],
      items: parsed.map((r) => ({ folderId: null, ...(r as Omit<SavedRequest, "folderId">) })),
    };
  }
  const c = parsed as Collection | null;
  return c && c.version === 2 ? c : emptyCollection();
}

// Local change feed: the UI subscribes so writes from MCP tools show up live.
const listeners = new Set<(c: Collection) => void>();

export function onCollectionChange(cb: (c: Collection) => void): () => void {
  listeners.add(cb);
  return () => void listeners.delete(cb);
}

export async function loadCollection(): Promise<Collection> {
  try {
    const raw = await H.storage.get(key());
    return raw ? migrate(JSON.parse(raw)) : emptyCollection();
  } catch {
    return emptyCollection();
  }
}

export async function saveCollection(c: Collection): Promise<void> {
  await H.storage.set(key(), JSON.stringify(c));
  listeners.forEach((l) => l(c));
}

export async function addToCollection(
  name: string,
  request: ClientRequest,
  folderId: string | null = null,
): Promise<Collection> {
  const c = await loadCollection();
  const next = {
    ...c,
    items: [...c.items, { id: crypto.randomUUID(), name, folderId, request: lighten(request) }],
  };
  await saveCollection(next);
  return next;
}

/** Overwrite a saved request in place; if it was deleted meanwhile, re-add it. */
export async function updateInCollection(
  id: string,
  name: string,
  request: ClientRequest,
): Promise<Collection> {
  const c = await loadCollection();
  const items = c.items.some((r) => r.id === id)
    ? c.items.map((r) => (r.id === id ? { ...r, request: lighten(request) } : r))
    : [...c.items, { id, name, folderId: null, request: lighten(request) }];
  const next = { ...c, items };
  await saveCollection(next);
  return next;
}

export async function createFolder(
  name: string,
  parentId: string | null = null,
): Promise<Collection> {
  const c = await loadCollection();
  const next = { ...c, folders: [...c.folders, { id: crypto.randomUUID(), name, parentId }] };
  await saveCollection(next);
  return next;
}

/** Rename a saved request or a folder (whichever the id matches). */
export async function renameItem(id: string, name: string): Promise<Collection> {
  const c = await loadCollection();
  const next = {
    ...c,
    folders: c.folders.map((f) => (f.id === id ? { ...f, name } : f)),
    items: c.items.map((r) => (r.id === id ? { ...r, name } : r)),
  };
  await saveCollection(next);
  return next;
}

/** The folder plus every folder nested under it. */
export function folderDescendants(c: Collection, folderId: string): Set<string> {
  const out = new Set<string>([folderId]);
  let grew = true;
  while (grew) {
    grew = false;
    for (const f of c.folders) {
      if (f.parentId && out.has(f.parentId) && !out.has(f.id)) {
        out.add(f.id);
        grew = true;
      }
    }
  }
  return out;
}

/** "parent / child" display path; "" for the root. */
export function folderPath(c: Collection, folderId: string | null): string {
  const parts: string[] = [];
  let cur = folderId;
  while (cur) {
    const f = c.folders.find((x) => x.id === cur);
    if (!f) break;
    parts.unshift(f.name);
    cur = f.parentId;
  }
  return parts.join(" / ");
}

/** Move a request into a folder (or root), or re-parent a folder. */
export async function moveItem(id: string, targetFolderId: string | null): Promise<Collection> {
  const c = await loadCollection();
  const folder = c.folders.find((f) => f.id === id);
  let next: Collection;
  if (folder) {
    if (targetFolderId && folderDescendants(c, id).has(targetFolderId)) {
      throw new Error("cannot move a folder into itself or its descendant");
    }
    next = {
      ...c,
      folders: c.folders.map((f) => (f.id === id ? { ...f, parentId: targetFolderId } : f)),
    };
  } else {
    next = {
      ...c,
      items: c.items.map((r) => (r.id === id ? { ...r, folderId: targetFolderId } : r)),
    };
  }
  await saveCollection(next);
  return next;
}

/** Delete a request, or a folder together with everything inside it. */
export async function deleteItem(id: string): Promise<Collection> {
  const c = await loadCollection();
  let next: Collection;
  if (c.folders.some((f) => f.id === id)) {
    const gone = folderDescendants(c, id);
    next = {
      ...c,
      folders: c.folders.filter((f) => !gone.has(f.id)),
      items: c.items.filter((r) => !r.folderId || !gone.has(r.folderId)),
    };
  } else {
    next = { ...c, items: c.items.filter((r) => r.id !== id) };
  }
  await saveCollection(next);
  return next;
}
```

- [ ] **Step 4: Run tests** — `pnpm test -- src/collections.test.ts`. Expected: PASS. (`HttpClientApp.tsx` now has type errors — fixed in Task 6; vitest doesn't typecheck the app, but do NOT run `pnpm build` yet.)

- [ ] **Step 5: Commit**

```bash
git add src/collections.ts src/collections.test.ts
git commit -m "feat(collections): v2 format — nested folders, rename/move/delete, change events"
```

---

### Task 3: Plugin — state.ts: draft & last-response pub/sub for MCP↔UI sync

**Files:**
- Modify: `~/claude-projects/trawl-plugin-http-client/src/state.ts`

**Interfaces:**
- Consumes: `ClientRequest` from `./model`, `SendResponse` from `./trawl`.
- Produces (used by Tasks 6, 8): existing `loadRequest`, `consumePending`, `subscribe` stay unchanged, plus:

```ts
export function publishDraft(req: ClientRequest): void      // UI → module (active tab request)
export function readDraft(): ClientRequest | null           // MCP get_draft: UI draft, else pending
export function publishResponse(r: SendResponse): void      // UI send + MCP send_request
export function readLastResponse(): SendResponse | null     // MCP get_last_response
```

- [ ] **Step 1: Append to `src/state.ts`** (no separate test — trivial accessors covered via Task 8's tests):

```ts
// ── MCP ↔ UI sync: the UI publishes its active draft and the latest response;
// MCP tools read them (and set_draft feeds loadRequest above). ──

import type { SendResponse } from "./trawl";

let uiDraft: ClientRequest | null = null;
let lastResponse: SendResponse | null = null;

export function publishDraft(req: ClientRequest): void {
  uiDraft = req;
}

/** The editor's current request; falls back to a not-yet-consumed pending one. */
export function readDraft(): ClientRequest | null {
  return uiDraft ?? pending;
}

export function publishResponse(r: SendResponse): void {
  lastResponse = r;
}

export function readLastResponse(): SendResponse | null {
  return lastResponse;
}
```

(Move the existing `import type { ClientRequest }` line if needed so all imports stay at the top.)

- [ ] **Step 2: Run** `pnpm test` — existing tests still PASS.

- [ ] **Step 3: Commit**

```bash
git add src/state.ts
git commit -m "feat(state): draft/last-response pub-sub for MCP tools"
```

---

### Task 4: Plugin — curl.ts: parseCurl + toCurl

**Files:**
- Create: `~/claude-projects/trawl-plugin-http-client/src/curl.ts`
- Create: `~/claude-projects/trawl-plugin-http-client/src/curl.test.ts`

**Interfaces:**
- Consumes: `blankRequest`, `parseUrl`, `toSendRequest`, `substitute`, types from `./model`; `EnvVar` from `./trawl`.
- Produces (used by Tasks 7, and optionally 8):

```ts
export function tokenizeShell(input: string): string[]
export function parseCurl(text: string): ClientRequest | null   // null when not a curl command
export function toCurl(req: ClientRequest, env: EnvVar[]): string
```

- [ ] **Step 1: Write the failing tests** — `src/curl.test.ts`:

```ts
import { describe, it, expect } from "vitest";
import { tokenizeShell, parseCurl, toCurl } from "./curl";
import { blankRequest, type ClientRequest } from "./model";

describe("tokenizeShell", () => {
  it("splits on whitespace and respects quotes", () => {
    expect(tokenizeShell(`curl -H 'X: a b' "u r l"`)).toEqual(["curl", "-H", "X: a b", "u r l"]);
  });
  it("handles escaped quotes inside double quotes and line continuations", () => {
    expect(tokenizeShell("curl \\\n  -d \"a=\\\"b\\\"\"")).toEqual(["curl", "-d", 'a="b"']);
  });
  it("handles $'...' strings with \\n escapes", () => {
    expect(tokenizeShell(`curl -d $'a\\nb'`)).toEqual(["curl", "-d", "a\nb"]);
  });
});

describe("parseCurl", () => {
  it("returns null for non-curl text", () => {
    expect(parseCurl("https://example.com")).toBeNull();
    expect(parseCurl("curly text")).toBeNull();
  });

  it("parses a bare GET with query params", () => {
    const r = parseCurl("curl 'https://api.io/v1/items?a=1&b=two words'")!;
    expect(r.method).toBe("GET");
    expect(r.url).toBe("https://api.io/v1/items");
    expect(r.params).toEqual([
      { key: "a", value: "1", enabled: true },
      { key: "b", value: "two words", enabled: true },
    ]);
  });

  it("parses Chrome-style: -H headers and --data-raw json → raw/json body, POST", () => {
    const r = parseCurl(
      `curl 'https://api.io/login' -H 'content-type: application/json' -H 'accept: */*' --data-raw '{"u":"a"}'`,
    )!;
    expect(r.method).toBe("POST");
    expect(r.bodyMode).toBe("raw");
    expect(r.rawType).toBe("json");
    expect(r.rawBody).toBe('{"u":"a"}');
    expect(r.headers).toContainEqual({ key: "accept", value: "*/*", enabled: true });
  });

  it("maps -d without content-type to form mode, joining multiple -d", () => {
    const r = parseCurl(`curl https://api.io -d a=1 -d 'b=c d'`)!;
    expect(r.bodyMode).toBe("form");
    expect(r.form).toEqual([
      { key: "a", value: "1", enabled: true },
      { key: "b", value: "c d", enabled: true },
    ]);
  });

  it("keeps a raw body for explicit non-form content types", () => {
    const r = parseCurl(`curl https://api.io -H 'Content-Type: text/plain' -d hello`)!;
    expect(r.bodyMode).toBe("raw");
    expect(r.rawType).toBe("text");
    expect(r.rawBody).toBe("hello");
  });

  it("parses -F into multipart text fields and @file parts", () => {
    const r = parseCurl(`curl https://api.io -F name=alice -F avatar=@photo.png`)!;
    expect(r.bodyMode).toBe("multipart");
    expect(r.multipartText).toEqual([{ key: "name", value: "alice", enabled: true }]);
    expect(r.multipartFiles).toHaveLength(1);
    expect(r.multipartFiles[0].key).toBe("avatar");
    expect(r.multipartFiles[0].fileName).toBe("photo.png");
    expect(r.multipartFiles[0].fileB64).toBe("");
  });

  it("honours -X, -u (basic auth) and -b (cookie)", () => {
    const r = parseCurl(`curl -X PUT https://api.io -u user:pass -b 'sid=1'`)!;
    expect(r.method).toBe("PUT");
    expect(r.headers).toContainEqual({ key: "Authorization", value: `Basic ${btoa("user:pass")}`, enabled: true });
    expect(r.headers).toContainEqual({ key: "Cookie", value: "sid=1", enabled: true });
  });

  it("takes the URL from --url and skips noise flags", () => {
    const r = parseCurl(`curl -s --compressed -o out.txt --connect-timeout 5 --url https://api.io/x`)!;
    expect(r.url).toBe("https://api.io/x");
    expect(r.method).toBe("GET");
  });
});

describe("toCurl", () => {
  const base: ClientRequest = {
    ...blankRequest(),
    method: "POST",
    url: "https://{{host}}/v1",
    params: [{ key: "q", value: "1", enabled: true }, { key: "off", value: "x", enabled: false }],
    headers: [{ key: "X-Token", value: "{{token}}", enabled: true }],
    bodyMode: "raw",
    rawType: "json",
    rawBody: '{"a":1}',
  };
  const env = [{ key: "host", value: "api.io" }, { key: "token", value: "T" }];

  it("substitutes env vars and produces a runnable command", () => {
    const cmd = toCurl(base, env);
    expect(cmd).toContain("curl -X POST 'https://api.io/v1?q=1'");
    expect(cmd).toContain("-H 'X-Token: T'");
    expect(cmd).toContain("-H 'Content-Type: application/json'");
    expect(cmd).toContain(`--data-raw '{"a":1}'`);
    expect(cmd).not.toContain("off=x");
  });

  it("omits -X for a bare GET and escapes single quotes", () => {
    const r = { ...blankRequest(), url: "https://a.io/it's" };
    const cmd = toCurl(r, []);
    expect(cmd.startsWith("curl 'https://a.io/it'\\''s'")).toBe(true);
    expect(cmd).not.toContain("-X GET");
  });

  it("renders multipart as -F flags (files by name)", () => {
    const r: ClientRequest = {
      ...blankRequest(),
      method: "POST",
      url: "https://a.io/up",
      bodyMode: "multipart",
      multipartText: [{ key: "n", value: "v", enabled: true }],
      multipartFiles: [{ key: "f", enabled: true, fileName: "a.png", fileB64: "", contentType: "image/png" }],
    };
    const cmd = toCurl(r, []);
    expect(cmd).toContain("-F 'n=v'");
    expect(cmd).toContain("-F 'f=@a.png'");
    expect(cmd).not.toContain("Content-Type: multipart");
  });

  it("round-trips through parseCurl", () => {
    const back = parseCurl(toCurl(base, env))!;
    expect(back.method).toBe("POST");
    expect(back.url).toBe("https://api.io/v1");
    expect(back.params).toEqual([{ key: "q", value: "1", enabled: true }]);
    expect(back.bodyMode).toBe("raw");
    expect(back.rawType).toBe("json");
    expect(back.rawBody).toBe('{"a":1}');
  });
});
```

- [ ] **Step 2: Run** `pnpm test -- src/curl.test.ts` — FAIL (module missing).

- [ ] **Step 3: Implement `src/curl.ts`:**

```ts
// cURL command ↔ ClientRequest. Import: paste a `curl ...` command into the URL
// bar. Export: "Copy as cURL" (env vars substituted so the command is runnable).

import {
  blankRequest,
  parseUrl,
  substitute,
  toSendRequest,
  type ClientRequest,
  type RawType,
  type Row,
} from "./model";
import type { EnvVar } from "./trawl";

/** Minimal POSIX-ish tokenizer: '', "", $'', backslash escapes, \<newline>. */
export function tokenizeShell(input: string): string[] {
  const src = input.replace(/\\\r?\n/g, " ");
  const out: string[] = [];
  let i = 0;
  while (i < src.length) {
    while (i < src.length && /\s/.test(src[i])) i++;
    if (i >= src.length) break;
    let tok = "";
    while (i < src.length && !/\s/.test(src[i])) {
      const ch = src[i];
      if (ch === "'") {
        i++;
        while (i < src.length && src[i] !== "'") tok += src[i++];
        i++;
      } else if (ch === '"') {
        i++;
        while (i < src.length && src[i] !== '"') {
          if (src[i] === "\\" && i + 1 < src.length && '"\\$`'.includes(src[i + 1])) {
            tok += src[i + 1];
            i += 2;
          } else {
            tok += src[i++];
          }
        }
        i++;
      } else if (ch === "$" && src[i + 1] === "'") {
        i += 2;
        const esc: Record<string, string> = { n: "\n", t: "\t", r: "\r" };
        while (i < src.length && src[i] !== "'") {
          if (src[i] === "\\" && i + 1 < src.length) {
            tok += esc[src[i + 1]] ?? src[i + 1];
            i += 2;
          } else {
            tok += src[i++];
          }
        }
        i++;
      } else if (ch === "\\") {
        if (i + 1 < src.length) tok += src[i + 1];
        i += 2;
      } else {
        tok += ch;
        i++;
      }
    }
    out.push(tok);
  }
  return out;
}

const row = (key: string, value: string): Row => ({ key, value, enabled: true });

// Flags whose value we consume and ignore.
const SKIP_WITH_VALUE = new Set([
  "-o", "--output", "-w", "--write-out", "--connect-timeout", "--max-time", "-m",
  "--retry", "--cacert", "--capath", "--cert", "--key", "-x", "--proxy", "-A",
  "--user-agent", "-e", "--referer",
]);

function decodePair(pair: string): Row {
  const eq = pair.indexOf("=");
  const dec = (s: string) => {
    try {
      return decodeURIComponent(s.replace(/\+/g, " "));
    } catch {
      return s;
    }
  };
  return eq === -1 ? row(dec(pair), "") : row(dec(pair.slice(0, eq)), dec(pair.slice(eq + 1)));
}

export function parseCurl(text: string): ClientRequest | null {
  const t = text.trim();
  if (!/^curl\s/i.test(t)) return null;
  const tokens = tokenizeShell(t).slice(1);

  let method: string | null = null;
  let url = "";
  const headers: Row[] = [];
  const dataParts: string[] = [];
  const formText: Row[] = [];
  const formFiles: { key: string; fileName: string }[] = [];

  for (let i = 0; i < tokens.length; i++) {
    let tok = tokens[i];
    let inlineVal: string | null = null;
    // curl accepts --long=value.
    if (tok.startsWith("--") && tok.includes("=")) {
      const eq = tok.indexOf("=");
      inlineVal = tok.slice(eq + 1);
      tok = tok.slice(0, eq);
    }
    const val = (): string => inlineVal ?? tokens[++i] ?? "";

    switch (tok) {
      case "-X":
      case "--request":
        method = val().toUpperCase();
        break;
      case "-H":
      case "--header": {
        const h = val();
        const c = h.indexOf(":");
        if (c > 0) headers.push(row(h.slice(0, c).trim(), h.slice(c + 1).trim()));
        break;
      }
      case "-d":
      case "--data":
      case "--data-raw":
      case "--data-binary":
      case "--data-ascii":
      case "--data-urlencode":
        dataParts.push(val());
        break;
      case "-F":
      case "--form": {
        const f = val();
        const eq = f.indexOf("=");
        if (eq > 0) {
          const k = f.slice(0, eq);
          const v = f.slice(eq + 1);
          if (v.startsWith("@")) formFiles.push({ key: k, fileName: v.slice(1) });
          else formText.push(row(k, v));
        }
        break;
      }
      case "-u":
      case "--user":
        headers.push(row("Authorization", `Basic ${btoa(val())}`));
        break;
      case "-b":
      case "--cookie":
        headers.push(row("Cookie", val()));
        break;
      case "--url":
        url = val();
        break;
      default:
        if (SKIP_WITH_VALUE.has(tok)) {
          val();
        } else if (!tok.startsWith("-") && !url) {
          url = tok;
        }
        // Other boolean flags (-s, --compressed, -k, -L, -v, ...) are ignored.
    }
  }
  if (!url) return null;

  const req: ClientRequest = { ...blankRequest() };
  const { base, params } = parseUrl(url);
  req.url = base;
  req.params = params;

  const ctHeader = headers.find((h) => h.key.toLowerCase() === "content-type");
  const ct = ctHeader?.value.toLowerCase() ?? "";

  if (formText.length || formFiles.length) {
    req.bodyMode = "multipart";
    req.multipartText = formText;
    req.multipartFiles = formFiles.map((f) => ({
      key: f.key,
      enabled: true,
      fileName: f.fileName,
      fileB64: "",
      contentType: "",
    }));
    // toSendRequest sets its own multipart content-type (with boundary).
    req.headers = headers.filter((h) => h !== ctHeader);
  } else if (dataParts.length) {
    const joined = dataParts.join("&");
    if (ct.includes("json")) {
      req.bodyMode = "raw";
      req.rawType = "json";
      req.rawBody = joined;
      req.headers = headers.filter((h) => h !== ctHeader); // rawType json re-adds it
    } else if (!ct || ct.includes("x-www-form-urlencoded")) {
      req.bodyMode = "form";
      req.form = joined.split("&").filter(Boolean).map(decodePair);
      req.headers = headers.filter((h) => h !== ctHeader);
    } else {
      req.bodyMode = "raw";
      req.rawType = "text";
      req.rawBody = joined;
      req.headers = headers; // explicit content-type stays as a header row
    }
  } else {
    req.headers = headers;
  }

  req.method = method ?? (req.bodyMode === "none" ? "GET" : "POST");
  return req;
}

const q = (s: string) => `'${s.replace(/'/g, `'\\''`)}'`;

/** Runnable curl command for the request; {{vars}} substituted from env. */
export function toCurl(req: ClientRequest, env: EnvVar[]): string {
  const wire = toSendRequest(req, env);
  const parts: string[] = ["curl"];
  if (req.method !== "GET") parts.push(`-X ${req.method}`);
  parts.push(q(wire.url));
  for (const [k, v] of wire.headers) {
    // -F builds its own multipart content-type; skip the boundary header.
    if (req.bodyMode === "multipart" && k.toLowerCase() === "content-type") continue;
    parts.push(`-H ${q(`${k}: ${v}`)}`);
  }
  if (req.bodyMode === "multipart") {
    for (const t of req.multipartText.filter((r) => r.enabled && r.key.trim())) {
      parts.push(`-F ${q(`${substitute(t.key, env)}=${substitute(t.value, env)}`)}`);
    }
    for (const f of req.multipartFiles.filter((r) => r.enabled && r.key.trim())) {
      parts.push(`-F ${q(`${f.key}=@${f.fileName || "file"}`)}`);
    }
  } else if (wire.body) {
    parts.push(`--data-raw ${q(wire.body)}`);
  }
  return parts.join(" \\\n  ");
}
```

Note: `substitute` and `parseUrl` are already exported from `model.ts`; `toSendRequest` handles env substitution for url/headers/body.

- [ ] **Step 4: Run** `pnpm test -- src/curl.test.ts` — PASS. Fix discrepancies by adjusting the implementation, not by weakening tests (the expected values above are the contract).

- [ ] **Step 5: Commit**

```bash
git add src/curl.ts src/curl.test.ts
git commit -m "feat(curl): parseCurl / toCurl with shell tokenizer"
```

---

### Task 5: Plugin — CollectionTree component (folders UI, rename, move, delete)

**Files:**
- Create: `~/claude-projects/trawl-plugin-http-client/src/CollectionTree.tsx`

**Interfaces:**
- Consumes: `Collection`, `SavedRequest`, `renameItem`, `moveItem`, `deleteItem`, `folderDescendants`, `folderPath` from `./collections`; `H.ui.MethodBadge`.
- Produces (used by Task 6):

```tsx
export function CollectionTree(props: {
  collection: Collection;
  onOpen: (s: SavedRequest) => void;
  onChanged: (c: Collection) => void;   // parent setState after any mutation
}): JSX.Element
```

No unit test (pure presentational; vitest here has no DOM env). Verified by typecheck/build and the manual checklist in Task 9.

- [ ] **Step 1: Implement `src/CollectionTree.tsx`:**

```tsx
import { useState } from "react";
import type { CSSProperties } from "react";
import {
  deleteItem,
  folderDescendants,
  folderPath,
  moveItem,
  renameItem,
  type Collection,
  type Folder,
  type SavedRequest,
} from "./collections";

const H = window.__TRAWL__!;

/** Nested folder/request tree with per-row ⋯ menu (rename / move / delete). */
export function CollectionTree({
  collection,
  onOpen,
  onChanged,
}: {
  collection: Collection;
  onOpen: (s: SavedRequest) => void;
  onChanged: (c: Collection) => void;
}) {
  const [closed, setClosed] = useState<Set<string>>(new Set()); // folders are open by default
  const [menuFor, setMenuFor] = useState<string | null>(null);
  const [renaming, setRenaming] = useState<{ id: string; value: string } | null>(null);
  const [armedDelete, setArmedDelete] = useState<string | null>(null);
  const { MethodBadge } = H.ui;

  const toggle = (id: string) =>
    setClosed((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });

  const closeMenus = () => {
    setMenuFor(null);
    setArmedDelete(null);
  };

  const commitRename = async () => {
    if (!renaming) return;
    const name = renaming.value.trim();
    if (name) onChanged(await renameItem(renaming.id, name));
    setRenaming(null);
  };

  const doMove = async (id: string, target: string | null) => {
    onChanged(await moveItem(id, target));
    closeMenus();
  };

  const doDelete = async (id: string) => {
    onChanged(await deleteItem(id));
    closeMenus();
  };

  /** Folders a row may move into (for folders: not itself/descendants). */
  const moveTargets = (id: string, isFolder: boolean): Folder[] => {
    if (!isFolder) return collection.folders;
    const banned = folderDescendants(collection, id);
    return collection.folders.filter((f) => !banned.has(f.id));
  };

  const renderMenu = (id: string, isFolder: boolean, currentParent: string | null) => (
    <div style={styles.menu}>
      <button style={styles.menuBtn} onClick={() => { setRenaming({ id, value: nameOf(id) }); closeMenus(); }}>
        Rename
      </button>
      <select
        style={styles.menuSelect}
        value={currentParent ?? ""}
        title="Move to folder"
        onChange={(e) => void doMove(id, e.target.value || null)}
      >
        <option value="">/ (root)</option>
        {moveTargets(id, isFolder).map((f) => (
          <option key={f.id} value={f.id}>{folderPath(collection, f.id)}</option>
        ))}
      </select>
      {armedDelete === id ? (
        <button style={{ ...styles.menuBtn, color: "var(--http-red)" }} onClick={() => void doDelete(id)}>
          {isFolder ? "Delete all?" : "Sure?"}
        </button>
      ) : (
        <button style={styles.menuBtn} onClick={() => setArmedDelete(id)}>Delete</button>
      )}
    </div>
  );

  const nameOf = (id: string): string =>
    collection.folders.find((f) => f.id === id)?.name ??
    collection.items.find((r) => r.id === id)?.name ?? "";

  const renderRename = (id: string) => (
    <input
      autoFocus
      style={{ ...styles.cell, flex: 1 }}
      value={renaming!.value}
      onChange={(e) => setRenaming({ id, value: e.target.value })}
      onKeyDown={(e) => {
        if (e.key === "Enter") void commitRename();
        if (e.key === "Escape") setRenaming(null);
      }}
      onBlur={() => void commitRename()}
    />
  );

  const renderRequest = (s: SavedRequest, depth: number) => (
    <div key={s.id} style={{ ...styles.row, paddingLeft: depth * 14 }}>
      {renaming?.id === s.id ? (
        renderRename(s.id)
      ) : (
        <button style={styles.load} title={s.name} onClick={() => onOpen(s)}>
          <MethodBadge method={s.request.method} />
          <span style={styles.name}>{s.name}</span>
        </button>
      )}
      <button style={styles.dots} title="Actions" onClick={() => setMenuFor(menuFor === s.id ? null : s.id)}>⋯</button>
      {menuFor === s.id && renderMenu(s.id, false, s.folderId)}
    </div>
  );

  const renderFolder = (f: Folder, depth: number) => (
    <div key={f.id}>
      <div style={{ ...styles.row, paddingLeft: depth * 14 }}>
        {renaming?.id === f.id ? (
          renderRename(f.id)
        ) : (
          <button style={styles.load} onClick={() => toggle(f.id)}>
            <span style={styles.chev}>{closed.has(f.id) ? "▸" : "▾"}</span>
            <span style={{ ...styles.name, fontWeight: 600 }}>{f.name}</span>
          </button>
        )}
        <button style={styles.dots} title="Actions" onClick={() => setMenuFor(menuFor === f.id ? null : f.id)}>⋯</button>
        {menuFor === f.id && renderMenu(f.id, true, f.parentId)}
      </div>
      {!closed.has(f.id) && renderChildren(f.id, depth + 1)}
    </div>
  );

  const renderChildren = (parentId: string | null, depth: number) => (
    <>
      {collection.folders.filter((f) => f.parentId === parentId).map((f) => renderFolder(f, depth))}
      {collection.items.filter((r) => r.folderId === parentId).map((r) => renderRequest(r, depth))}
    </>
  );

  if (collection.folders.length === 0 && collection.items.length === 0) return <></>;
  return <div>{renderChildren(null, 0)}</div>;
}

const styles: Record<string, CSSProperties> = {
  row: { position: "relative", display: "flex", alignItems: "center", gap: 4 },
  load: { flex: 1, minWidth: 0, display: "flex", alignItems: "center", gap: 6, border: "none", background: "transparent", padding: "4px 4px", cursor: "pointer", textAlign: "left", borderRadius: 6 },
  name: { overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", fontSize: 12, color: "var(--foreground)" },
  chev: { fontSize: 10, color: "var(--muted-foreground)", width: 10 },
  dots: { border: "none", background: "transparent", color: "var(--muted-foreground)", cursor: "pointer", fontSize: 14, padding: "0 4px", borderRadius: 4 },
  menu: { position: "absolute", right: 0, top: "100%", zIndex: 10, display: "flex", flexDirection: "column", gap: 4, background: "var(--card)", border: "1px solid var(--border)", borderRadius: 8, padding: 6, minWidth: 140, boxShadow: "0 4px 14px rgba(0,0,0,.25)" },
  menuBtn: { border: "1px solid var(--border)", background: "transparent", color: "var(--foreground)", borderRadius: 6, padding: "4px 8px", fontSize: 12, cursor: "pointer", textAlign: "left" },
  menuSelect: { border: "1px solid var(--border)", background: "var(--card)", color: "var(--foreground)", borderRadius: 6, padding: "4px 6px", fontSize: 12 },
  cell: { border: "1px solid var(--border)", background: "var(--card)", color: "var(--foreground)", borderRadius: 6, padding: "6px 8px", fontSize: 12, fontFamily: "ui-monospace, monospace", minWidth: 0 },
};
```

- [ ] **Step 2: Typecheck** — `pnpm exec tsc --noEmit`. Expected: CollectionTree itself clean (HttpClientApp errors from Task 2's API change remain until Task 6 — confirm the only errors are in `HttpClientApp.tsx`).

- [ ] **Step 3: Commit**

```bash
git add src/CollectionTree.tsx
git commit -m "feat(ui): CollectionTree — nested folders, rename, move, two-click delete"
```

---

### Task 6: Plugin — HttpClientApp integration: tree, save-to-folder, proxy switch, live sync

**Files:**
- Modify: `~/claude-projects/trawl-plugin-http-client/src/HttpClientApp.tsx`

**Interfaces:**
- Consumes: `Collection`, `emptyCollection`, `addToCollection`, `updateInCollection`, `createFolder`, `onCollectionChange`, `folderPath` from `./collections`; `CollectionTree` from `./CollectionTree`; `publishDraft`, `publishResponse` from `./state`.
- Produces: working UI; `viaProxy` persisted at `httpclient.viaProxy` ("1"/"0").

- [ ] **Step 1: Update state and imports.** In `HttpClientApp.tsx`:

Replace the collections import block with:

```ts
import {
  addToCollection,
  createFolder,
  emptyCollection,
  folderPath,
  loadCollection,
  onCollectionChange,
  updateInCollection,
  type Collection,
  type SavedRequest,
} from "./collections";
import { CollectionTree } from "./CollectionTree";
import { consumePending, publishDraft, publishResponse, subscribe } from "./state";
```

Replace `const [collection, setCollection] = useState<SavedRequest[]>([]);` with:

```ts
  const [collection, setCollection] = useState<Collection>(emptyCollection());
  const [saveFolderId, setSaveFolderId] = useState<string | null>(null);
  const [newFolderName, setNewFolderName] = useState<string | null>(null);
```

- [ ] **Step 2: Live sync + draft publication.** In the main `useEffect`, add a collection-change subscription; add a draft-publishing effect after it:

```ts
  useEffect(() => {
    void loadCollection().then(setCollection);
    const offReq = subscribe((r) => openTab(newTab(r)));
    const offColl = onCollectionChange(setCollection); // MCP writes show up live
    const offProj = H.projects.onChange((p) => {
      setProject(p);
      setEnv(p?.env ?? []);
      setEnvDirty(false);
      void loadCollection().then(setCollection);
    });
    return () => {
      offReq();
      offColl();
      offProj();
    };
  }, []);

  // Expose the active draft to MCP get_draft.
  useEffect(() => {
    publishDraft(req);
  }, [req]);
```

- [ ] **Step 3: Response publication.** In `send`, after `patchTab(tabId, { res: r, sending: false });` add:

```ts
      publishResponse(r);
```

- [ ] **Step 4: Save flow with folder choice + folder creation.** Replace `confirmSave` and add `confirmNewFolder`:

```ts
  const confirmSave = async () => {
    const name = (saveName ?? "").trim();
    if (!name) {
      setSaveName(null);
      return;
    }
    const next = await addToCollection(name, req, saveFolderId);
    setCollection(next);
    const added = next.items[next.items.length - 1];
    patchTab(active.id, { savedId: added.id, name: added.name });
    setSaveName(null);
  };

  const confirmNewFolder = async () => {
    const name = (newFolderName ?? "").trim();
    if (name) setCollection(await createFolder(name));
    setNewFolderName(null);
  };
```

Update `saveActive`'s update branch (result shape changed):

```ts
    setCollection(await updateInCollection(active.savedId, active.name ?? "Request", req));
```

(unchanged call — only the state type changed). Update the `dirty` lookup:

```ts
  const savedEntry = active.savedId ? collection.items.find((c) => c.id === active.savedId) : undefined;
```

- [ ] **Step 5: Collection section markup.** Replace the `<Section title={...Collection...}>` contents:

```tsx
        <Section
          title={project ? `Collection · ${project.name}` : "Collection"}
          action={{ label: "+ Folder", onClick: () => setNewFolderName("") }}
        >
          {newFolderName !== null && (
            <div style={{ display: "flex", gap: 6, marginBottom: 8 }}>
              <input
                autoFocus
                style={{ ...styles.cell, flex: 1 }}
                value={newFolderName}
                placeholder="Folder name…"
                onChange={(e) => setNewFolderName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") void confirmNewFolder();
                  if (e.key === "Escape") setNewFolderName(null);
                }}
              />
              <button style={styles.sectionAction} onClick={() => void confirmNewFolder()}>Add</button>
            </div>
          )}
          {saveName !== null && (
            <div style={{ display: "flex", flexDirection: "column", gap: 6, marginBottom: 8 }}>
              <input
                autoFocus
                style={styles.cell}
                value={saveName}
                placeholder="Request name…"
                onChange={(e) => setSaveName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") void confirmSave();
                  if (e.key === "Escape") setSaveName(null);
                }}
              />
              <div style={{ display: "flex", gap: 6 }}>
                <select
                  style={{ ...styles.cell, flex: 1 }}
                  value={saveFolderId ?? ""}
                  title="Folder"
                  onChange={(e) => setSaveFolderId(e.target.value || null)}
                >
                  <option value="">/ (root)</option>
                  {collection.folders.map((f) => (
                    <option key={f.id} value={f.id}>{folderPath(collection, f.id)}</option>
                  ))}
                </select>
                <button style={styles.sectionAction} onClick={() => void confirmSave()}>Save</button>
              </div>
            </div>
          )}
          {collection.folders.length === 0 && collection.items.length === 0 ? (
            <Muted>No saved requests.</Muted>
          ) : (
            <CollectionTree collection={collection} onOpen={openSaved} onChanged={setCollection} />
          )}
        </Section>
```

Note: the old `+ Save` section action is replaced by `+ Folder` — saving now always goes through the `Save…`/`Save` button in the request bar (`beginSave` stays wired there). Remove the now-unused `collRow`/`collLoad`/`collName` styles and the `removeFromCollection` import.

- [ ] **Step 6: Proxy switch + persistence.** Replace the `viaProxy` state/label:

```ts
  const [viaProxy, setViaProxyState] = useState(false);
  // ...
  useEffect(() => {
    void H.storage.get("httpclient.viaProxy").then((v) => setViaProxyState(v === "1"));
  }, []);
  const setViaProxy = (on: boolean) => {
    setViaProxyState(on);
    void H.storage.set("httpclient.viaProxy", on ? "1" : "0");
  };
```

Replace the `<label style={styles.toggle}>…</label>` block in the bar with:

```tsx
          <label style={styles.toggle} title="Route through the local proxy (also captured). Starts the proxy if it's stopped.">
            <Switch on={viaProxy} onChange={setViaProxy} />
            via proxy
          </label>
```

Add the `Switch` component next to the other small helpers:

```tsx
function Switch({ on, onChange }: { on: boolean; onChange: (on: boolean) => void }) {
  return (
    <button
      role="switch"
      aria-checked={on}
      onClick={() => onChange(!on)}
      style={{
        width: 30, height: 18, borderRadius: 9, border: "1px solid var(--border)", padding: 1,
        background: on ? "var(--primary)" : "var(--card)", cursor: "pointer", flexShrink: 0,
        display: "flex", alignItems: "center", transition: "background .15s",
      }}
    >
      <span
        style={{
          width: 14, height: 14, borderRadius: 7, background: on ? "var(--primary-foreground)" : "var(--muted-foreground)",
          transform: on ? "translateX(12px)" : "translateX(0)", transition: "transform .15s",
        }}
      />
    </button>
  );
}
```

- [ ] **Step 7: Typecheck + tests** — `pnpm exec tsc --noEmit && pnpm test`. Expected: clean, all PASS.

- [ ] **Step 8: Commit**

```bash
git add src/HttpClientApp.tsx
git commit -m "feat(ui): folder tree + save-to-folder, proxy switch with persistence, MCP live sync"
```

---

### Task 7: Plugin — cURL paste-import and Copy-as-cURL

**Files:**
- Modify: `~/claude-projects/trawl-plugin-http-client/src/VarInput.tsx` (new optional prop)
- Modify: `~/claude-projects/trawl-plugin-http-client/src/HttpClientApp.tsx` (wire up)

**Interfaces:**
- Consumes: `parseCurl`, `toCurl` from `./curl` (Task 4).
- Produces: `VarInput` prop `onPasteText?: (text: string) => boolean` — return `true` to consume the paste.

- [ ] **Step 1: VarInput prop.** In `VarInput.tsx` add to the props type and destructuring: `onPasteText?: (text: string) => boolean;` and on the `<input>` element add:

```tsx
      onPaste={(e) => {
        if (onPasteText?.(e.clipboardData.getData("text"))) e.preventDefault();
      }}
```

- [ ] **Step 2: Wire into the URL bar.** In `HttpClientApp.tsx` add imports:

```ts
import { parseCurl, toCurl } from "./curl";
```

Add state + handlers inside the component:

```ts
  const [curlCopied, setCurlCopied] = useState(false);
  const [curlImported, setCurlImported] = useState(false);

  /** Paste of a whole `curl ...` command replaces the active request. */
  const onUrlPaste = (text: string): boolean => {
    const parsed = parseCurl(text);
    if (!parsed) return false;
    patchTab(active.id, { req: parsed, res: null });
    setCurlImported(true);
    setTimeout(() => setCurlImported(false), 2000);
    return true;
  };

  const copyCurl = () => {
    void navigator.clipboard.writeText(toCurl(req, env));
    setCurlCopied(true);
    setTimeout(() => setCurlCopied(false), 1500);
  };
```

Pass the handler to the URL `VarInput`: add `onPasteText={onUrlPaste}` to its props. Add the export button in the bar, before the Save button:

```tsx
          <button style={styles.saveBar} title="Copy as cURL" onClick={copyCurl}>
            {curlCopied ? "Copied ✓" : "cURL"}
          </button>
```

Show import feedback in the warn area (before the unknown-variables warning):

```tsx
        {curlImported && <div style={{ ...styles.warn, color: "var(--primary)" }}>Imported from cURL</div>}
```

- [ ] **Step 3: Typecheck + tests** — `pnpm exec tsc --noEmit && pnpm test`. Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add src/VarInput.tsx src/HttpClientApp.tsx
git commit -m "feat(curl): paste-import in the URL bar + Copy as cURL button"
```

---

### Task 8: Plugin — MCP tools (10) + registration + trawl.d.ts

**Files:**
- Create: `~/claude-projects/trawl-plugin-http-client/src/mcpTools.ts`
- Create: `~/claude-projects/trawl-plugin-http-client/src/mcpTools.test.ts`
- Modify: `~/claude-projects/trawl-plugin-http-client/src/trawl.d.ts` (add `mcp` to `TrawlHost`)
- Modify: `~/claude-projects/trawl-plugin-http-client/src/plugin.tsx` (register at init)

**Interfaces:**
- Consumes: collections API (Task 2), `loadRequest`/`readDraft`/`readLastResponse`/`publishResponse` (Task 3), `toSendRequest`/`blankRequest` from `./model`.
- Produces: `export function registerMcpTools(host: TrawlHost): void` — registers all 10 tools; exported pure helpers `requestFromArgs`, `truncateResponse` for tests.

- [ ] **Step 1: Extend `trawl.d.ts`.** Add to the `TrawlHost` interface (after `storage`):

```ts
  mcp: {
    /** Register an MCP tool `<pluginId>_<name>`. Init-time only. */
    registerTool(spec: {
      name: string;
      description: string;
      inputSchema: Record<string, unknown>;
      handler: (args: unknown) => unknown | Promise<unknown>;
      timeoutMs?: number;
    }): Promise<void>;
    unregisterTool(name: string): Promise<void>;
  };
```

- [ ] **Step 2: Write the failing tests** — `src/mcpTools.test.ts`:

```ts
import { describe, it, expect, beforeEach } from "vitest";
import { blankRequest } from "./model";

// Shared window stub (same pattern as collections.test.ts) + capture registered tools.
const store = new Map<string, string>();
const sent: { req: unknown; viaProxy: boolean }[] = [];
let sendResult = { status: 200, headers: [], body: "ok", bodyIsText: true, durationMs: 5, error: null };
const host = {
  projects: { active: () => ({ id: "p1", name: "P", env: [{ key: "host", value: "api.io" }] }) },
  storage: {
    get: async (k: string) => store.get(k) ?? null,
    set: async (k: string, v: string) => void store.set(k, v),
  },
  http: {
    send: async (req: unknown, viaProxy = false) => {
      sent.push({ req, viaProxy });
      return sendResult;
    },
  },
  mcp: { registerTool: async (spec: ToolSpec) => void tools.set(spec.name, spec) },
  setMode: () => {},
};
(globalThis as { window?: unknown }).window = { __TRAWL__: host };

interface ToolSpec {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
  handler: (args: unknown) => unknown | Promise<unknown>;
}
const tools = new Map<string, ToolSpec>();

const { registerMcpTools, requestFromArgs, truncateResponse } = await import("./mcpTools");
const { loadCollection } = await import("./collections");

registerMcpTools(window.__TRAWL__!);
const call = (name: string, args: unknown = {}) => tools.get(name)!.handler(args);

beforeEach(() => {
  store.clear();
  sent.length = 0;
});

describe("registration", () => {
  it("registers all 10 tools", () => {
    expect([...tools.keys()].sort()).toEqual([
      "create_folder", "delete_item", "get_draft", "get_last_response", "get_request",
      "list_collection", "save_request", "send_request", "set_draft", "update_item",
    ]);
  });
});

describe("requestFromArgs", () => {
  it("builds a ClientRequest with defaults and normalized rows", () => {
    const r = requestFromArgs({ url: "https://a.io", headers: [{ key: "X", value: "1" }] });
    expect(r.method).toBe("GET");
    expect(r.headers).toEqual([{ key: "X", value: "1", enabled: true }]);
    expect(r.bodyMode).toBe("none");
  });
  it("overlays args onto a base request", () => {
    const base = { ...blankRequest(), method: "POST", url: "https://a.io", rawBody: "x", bodyMode: "raw" as const };
    const r = requestFromArgs({ rawBody: "y" }, base);
    expect(r.method).toBe("POST");
    expect(r.rawBody).toBe("y");
  });
});

describe("truncateResponse", () => {
  it("passes small bodies through untouched", () => {
    expect(truncateResponse({ ...sendResult, body: "short" }).truncated).toBeUndefined();
  });
  it("cuts bodies over 200k chars and flags them", () => {
    const r = truncateResponse({ ...sendResult, body: "a".repeat(200_001) });
    expect(r.truncated).toBe(true);
    expect(r.body.length).toBe(200_000);
  });
});

describe("collection tools", () => {
  it("save_request → list_collection → get_request round trip", async () => {
    const saved = (await call("save_request", { name: "r1", method: "POST", url: "https://a.io" })) as { id: string };
    const list = (await call("list_collection")) as { items: { id: string; name: string; method: string }[] };
    expect(list.items).toHaveLength(1);
    expect(list.items[0].name).toBe("r1");
    expect(list.items[0].method).toBe("POST");
    const full = (await call("get_request", { id: saved.id })) as { request: { url: string } };
    expect(full.request.url).toBe("https://a.io");
  });

  it("save_request with id updates; create_folder/update_item/delete_item manage the tree", async () => {
    const f = (await call("create_folder", { name: "api" })) as { id: string };
    const saved = (await call("save_request", { name: "r", url: "https://a.io", folderId: f.id })) as { id: string };
    await call("save_request", { id: saved.id, url: "https://b.io" });
    await call("update_item", { id: saved.id, name: "renamed" });
    let c = await loadCollection();
    expect(c.items[0].request.url).toBe("https://b.io");
    expect(c.items[0].name).toBe("renamed");
    expect(c.items[0].folderId).toBe(f.id);
    await call("delete_item", { id: f.id });
    c = await loadCollection();
    expect(c.folders).toEqual([]);
    expect(c.items).toEqual([]);
  });

  it("get_request throws a readable error for unknown ids", async () => {
    await expect(call("get_request", { id: "nope" })).rejects.toThrow(/no saved request/);
  });
});

describe("send_request", () => {
  it("sends an inline request with env substitution and returns the response", async () => {
    const res = (await call("send_request", { url: "https://{{host}}/v1", viaProxy: true })) as { status: number };
    expect(res.status).toBe(200);
    expect(sent[0].viaProxy).toBe(true);
    expect((sent[0].req as { url: string }).url).toBe("https://api.io/v1");
  });

  it("sends a saved request by id", async () => {
    const saved = (await call("save_request", { name: "r", method: "POST", url: "https://a.io" })) as { id: string };
    await call("send_request", { savedId: saved.id });
    expect((sent[0].req as { method: string }).method).toBe("POST");
  });

  it("requires url or savedId", async () => {
    await expect(call("send_request", {})).rejects.toThrow(/url or savedId/);
  });

  it("feeds get_last_response", async () => {
    await call("send_request", { url: "https://a.io" });
    const last = (await call("get_last_response")) as { status: number };
    expect(last.status).toBe(200);
  });
});

describe("draft tools", () => {
  it("set_draft then get_draft round trips", async () => {
    await call("set_draft", { method: "PUT", url: "https://d.io" });
    const d = (await call("get_draft")) as { method: string; url: string };
    expect(d.method).toBe("PUT");
    expect(d.url).toBe("https://d.io");
  });
});
```

- [ ] **Step 3: Run** `pnpm test -- src/mcpTools.test.ts` — FAIL (module missing).

- [ ] **Step 4: Implement `src/mcpTools.ts`:**

```ts
// MCP tools: the full client surface (create/save/send/analyze) exposed to
// agents via the host bridge. Headless — they work with storage and H.http
// directly; an open UI picks changes up through collections/state pub-sub.

import {
  addToCollection,
  createFolder,
  deleteItem,
  loadCollection,
  moveItem,
  renameItem,
  updateInCollection,
} from "./collections";
import { blankRequest, toSendRequest, type BodyMode, type ClientRequest, type RawType } from "./model";
import { loadRequest, publishResponse, readDraft, readLastResponse } from "./state";
import type { SendResponse, TrawlHost } from "./trawl";

const MAX_BODY = 200_000;

interface RowArg {
  key: string;
  value: string;
  enabled?: boolean;
}

export interface RequestArgs {
  method?: string;
  url?: string;
  params?: RowArg[];
  headers?: RowArg[];
  bodyMode?: BodyMode;
  rawBody?: string;
  rawType?: RawType;
  form?: RowArg[];
}

const rows = (rs: RowArg[] | undefined) =>
  (rs ?? []).map((r) => ({ key: r.key, value: r.value, enabled: r.enabled !== false }));

/** Build a ClientRequest from tool args, optionally overlaying a base (saved) request. */
export function requestFromArgs(a: RequestArgs, base?: ClientRequest): ClientRequest {
  const b = base ?? blankRequest();
  const req: ClientRequest = {
    ...b,
    method: (a.method ?? b.method).toUpperCase(),
    url: a.url ?? b.url,
    params: a.params ? rows(a.params) : b.params,
    headers: a.headers ? rows(a.headers) : b.headers,
    rawBody: a.rawBody ?? b.rawBody,
    rawType: a.rawType ?? b.rawType,
    form: a.form ? rows(a.form) : b.form,
  };
  req.bodyMode = a.bodyMode ?? (a.rawBody !== undefined ? "raw" : a.form ? "form" : b.bodyMode);
  return req;
}

export function truncateResponse(r: SendResponse): SendResponse & { truncated?: boolean } {
  if (r.body.length <= MAX_BODY) return r;
  return { ...r, body: r.body.slice(0, MAX_BODY), truncated: true };
}

// ── JSON Schemas ──

const rowSchema = {
  type: "object",
  properties: {
    key: { type: "string" },
    value: { type: "string" },
    enabled: { type: "boolean" },
  },
  required: ["key", "value"],
};

const requestProps = {
  method: { type: "string", description: "HTTP method (default GET)" },
  url: { type: "string", description: "Base URL; query goes into params. {{vars}} allowed." },
  params: { type: "array", items: rowSchema, description: "Query parameters" },
  headers: { type: "array", items: rowSchema },
  bodyMode: { type: "string", enum: ["none", "raw", "form", "multipart"] },
  rawBody: { type: "string", description: "Raw body (sets bodyMode=raw if bodyMode omitted)" },
  rawType: { type: "string", enum: ["text", "json", "xml", "html"], description: "Content-Type for raw bodies" },
  form: { type: "array", items: rowSchema, description: "URL-encoded form fields" },
};

async function mustGetItem(id: string) {
  const c = await loadCollection();
  const item = c.items.find((i) => i.id === id);
  if (!item) throw new Error(`no saved request with id "${id}"`);
  return { c, item };
}

export function registerMcpTools(host: TrawlHost): void {
  const reg = (spec: Parameters<TrawlHost["mcp"]["registerTool"]>[0]) => void host.mcp.registerTool(spec);

  reg({
    name: "send_request",
    description:
      "Send an HTTP request (inline fields or a saved request by savedId) and return the response. " +
      "Env vars {{name}} of the active project are substituted unless substituteEnv=false. " +
      "viaProxy=true routes through the capture proxy (auto-started when stopped).",
    inputSchema: {
      type: "object",
      properties: {
        ...requestProps,
        savedId: { type: "string", description: "Send a saved request (inline fields override its parts)" },
        viaProxy: { type: "boolean", description: "Route through the local capture proxy (default false)" },
        substituteEnv: { type: "boolean", description: "Substitute {{vars}} from the active project env (default true)" },
      },
    },
    timeoutMs: 45_000,
    handler: async (args) => {
      const a = args as RequestArgs & { savedId?: string; viaProxy?: boolean; substituteEnv?: boolean };
      let req: ClientRequest;
      if (a.savedId) {
        req = requestFromArgs(a, (await mustGetItem(a.savedId)).item.request);
      } else {
        if (!a.url) throw new Error("url or savedId is required");
        req = requestFromArgs(a);
      }
      const env = a.substituteEnv === false ? [] : host.projects.active()?.env ?? [];
      const res = await host.http.send(toSendRequest(req, env), a.viaProxy === true);
      publishResponse(res);
      return truncateResponse(res);
    },
  });

  reg({
    name: "list_collection",
    description: "List the active project's collection: folder tree and saved requests (summary).",
    inputSchema: { type: "object", properties: {} },
    handler: async () => {
      const c = await loadCollection();
      return {
        folders: c.folders,
        items: c.items.map((i) => ({
          id: i.id,
          name: i.name,
          folderId: i.folderId,
          method: i.request.method,
          url: i.request.url,
        })),
      };
    },
  });

  reg({
    name: "get_request",
    description: "Get a saved request in full by id.",
    inputSchema: { type: "object", properties: { id: { type: "string" } }, required: ["id"] },
    handler: async (args) => (await mustGetItem((args as { id: string }).id)).item,
  });

  reg({
    name: "save_request",
    description:
      "Save a request to the collection. Without id: creates (name required, optional folderId). " +
      "With id: updates the saved request's fields (inline fields override; folderId moves it).",
    inputSchema: {
      type: "object",
      properties: {
        ...requestProps,
        id: { type: "string", description: "Existing saved request to update" },
        name: { type: "string" },
        folderId: { type: "string", description: "Target folder (omit = root for new, keep for update)" },
      },
    },
    handler: async (args) => {
      const a = args as RequestArgs & { id?: string; name?: string; folderId?: string | null };
      if (a.id) {
        const { item } = await mustGetItem(a.id);
        await updateInCollection(a.id, a.name ?? item.name, requestFromArgs(a, item.request));
        if (a.name && a.name !== item.name) await renameItem(a.id, a.name);
        if (a.folderId !== undefined && a.folderId !== item.folderId) await moveItem(a.id, a.folderId);
        return { id: a.id };
      }
      if (!a.name) throw new Error("name is required for a new request");
      if (!a.url) throw new Error("url is required for a new request");
      const next = await addToCollection(a.name, requestFromArgs(a), a.folderId ?? null);
      return { id: next.items[next.items.length - 1].id };
    },
  });

  reg({
    name: "update_item",
    description: "Rename and/or move a saved request or folder. folderId: target folder (null/empty = root).",
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string" },
        name: { type: "string" },
        folderId: { type: ["string", "null"] },
      },
      required: ["id"],
    },
    handler: async (args) => {
      const a = args as { id: string; name?: string; folderId?: string | null };
      if (a.name) await renameItem(a.id, a.name);
      if (a.folderId !== undefined) await moveItem(a.id, a.folderId || null);
      return { ok: true };
    },
  });

  reg({
    name: "delete_item",
    description: "Delete a saved request, or a folder recursively (all nested folders and requests).",
    inputSchema: { type: "object", properties: { id: { type: "string" } }, required: ["id"] },
    handler: async (args) => {
      await deleteItem((args as { id: string }).id);
      return { ok: true };
    },
  });

  reg({
    name: "create_folder",
    description: "Create a collection folder (optionally nested under parentId).",
    inputSchema: {
      type: "object",
      properties: { name: { type: "string" }, parentId: { type: "string" } },
      required: ["name"],
    },
    handler: async (args) => {
      const a = args as { name: string; parentId?: string };
      const next = await createFolder(a.name, a.parentId ?? null);
      return { id: next.folders[next.folders.length - 1].id };
    },
  });

  reg({
    name: "set_draft",
    description:
      "Load a request into the HTTP Client editor (opens a new editor tab when the UI is visible; " +
      "otherwise it's picked up when the tab opens). focus=true also switches the app to the HTTP Client mode.",
    inputSchema: {
      type: "object",
      properties: { ...requestProps, focus: { type: "boolean" } },
      required: ["url"],
    },
    handler: (args) => {
      const a = args as RequestArgs & { focus?: boolean };
      loadRequest(requestFromArgs(a));
      if (a.focus) host.setMode("http-client");
      return { ok: true };
    },
  });

  reg({
    name: "get_draft",
    description: "Read the request currently in the HTTP Client editor (null if none).",
    inputSchema: { type: "object", properties: {} },
    handler: () => readDraft(),
  });

  reg({
    name: "get_last_response",
    description: "The most recent response received in the client (UI send or send_request tool). Null if none.",
    inputSchema: { type: "object", properties: {} },
    handler: () => {
      const r = readLastResponse();
      return r ? truncateResponse(r) : null;
    },
  });
}
```

- [ ] **Step 5: Register in `plugin.tsx`.** Add import `import { registerMcpTools } from "./mcpTools";` and inside the `if (host) { ... }` block (after `registerFlowAction`):

```ts
  registerMcpTools(host);
```

- [ ] **Step 6: Run** `pnpm test` — all files PASS. Note: `set_draft` → `get_draft` works because `loadRequest` sets `pending` and `readDraft()` falls back to it when no UI has published a draft.

- [ ] **Step 7: Typecheck** — `pnpm exec tsc --noEmit`. Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add src/mcpTools.ts src/mcpTools.test.ts src/trawl.d.ts src/plugin.tsx
git commit -m "feat(mcp): 10 tools — send, collection CRUD, folders, draft, last response"
```

---

### Task 9: Plugin — version 0.5.0, README, build, manual verification

**Files:**
- Modify: `~/claude-projects/trawl-plugin-http-client/package.json` (version)
- Modify: `~/claude-projects/trawl-plugin-http-client/trawl-plugin.json` (version, apiVersion)
- Modify: `~/claude-projects/trawl-plugin-http-client/README.md`
- Modify: `~/claude-projects/trawl-plugin-http-client/dist/plugin.js` (build artifact — committed in this repo)

- [ ] **Step 1: Bump versions.** `package.json`: `"version": "0.5.0"`. `trawl-plugin.json`: `"version": "0.5.0"`, `"apiVersion": "1.5.0"`.

- [ ] **Step 2: README.** Add sections (append after existing feature description):

```markdown
## Collections & folders

Requests are saved per project. Folders nest to any depth; use the ⋯ menu on a
row to rename, move, or delete (folder delete removes everything inside).

## cURL

- **Import:** paste a whole `curl ...` command into the URL bar — method,
  headers, and body are filled in (`-F @file` parts need re-picking the file).
- **Export:** the `cURL` button copies the current request as a runnable
  command with `{{vars}}` substituted.

## Via proxy

The **via proxy** switch routes sends through Trawl's capture proxy so they
show up in traffic. If the proxy is stopped, the host starts it automatically.

## MCP tools

When Trawl's MCP server is enabled, the plugin registers tools under
`http-client_*`: `send_request`, `list_collection`, `get_request`,
`save_request`, `update_item`, `delete_item`, `create_folder`, `set_draft`,
`get_draft`, `get_last_response` — enough for an agent to build, save,
organize, send, and analyze requests end to end.
```

- [ ] **Step 3: Full test + build**

Run: `pnpm test && pnpm build`
Expected: all tests PASS; `dist/plugin.js` rebuilt without errors.

- [ ] **Step 4: Manual verification (host app).** Merge the worktree branches (plugin → its main; host → its main, per the user's worktree→merge workflow), run the host app (`cd ~/claude-projects/http-catch && pnpm tauri dev`), reload the plugin, and check:
  1. Old saved requests still appear (v1 migration) and open.
  2. Create a folder, save a request into it, rename both, move the request to root, delete the folder.
  3. Toggle **via proxy** on with the proxy stopped → Send → response arrives, topbar shows the proxy running, request appears in traffic.
  4. Paste a Chrome "Copy as cURL" command into the URL bar → request fills in; `cURL` button copies a runnable command.
  5. With MCP connected (e.g. from Claude Code): `list_collection`, `save_request`, `send_request`, `get_last_response` work; a `save_request` from MCP appears in the open UI without reload; `set_draft` with `focus: true` opens a tab.

- [ ] **Step 5: Commit + merge**

```bash
git add package.json trawl-plugin.json README.md dist/
git commit -m "chore: v0.5.0 — folders, cURL import/export, proxy switch, MCP tools"
```

Then merge both branches to their mains (no PR needed unless the user asks).
