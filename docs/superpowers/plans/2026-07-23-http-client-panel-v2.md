# HTTP Client v0.6.0 — Panel v2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Resizable left panel with Collection | History | Env tabs, search in collection (filtered tree) and history, and persistent per-project history (5000 entries, no response bodies).

**Architecture:** All work in `~/claude-projects/trawl-plugin-http-client` (worktree branch off `b40154f`). New `history.ts` module mirrors `collections.ts` (storage + change feed). Pure `filterCollection` in `collections.ts` drives tree filtering. `HttpClientApp.tsx` left column is rebuilt around a tab switcher and a pointer-drag resize handle.

**Tech Stack:** TypeScript, React 19 (inline styles), vitest, `H.storage` for persistence.

## Global Constraints

- Worktree workflow: branch `feature/v0.6` in `.claude/worktrees/v0.6`, merge to main when green, push + tag `v0.6.0`.
- Storage keys: `httpclient.history.<projectId|_global>`, `httpclient.panelWidth` (global), `httpclient.panelTab` (global).
- History cap **5000**; UI renders at most **200** matching rows («showing 200 of N»). No response bodies stored (only `status`).
- Panel width 180–520px, default 250.
- No new dependencies; no `window.confirm` (two-click confirmation pattern).
- Version → **0.6.0** in `package.json` + `trawl-plugin.json`; `apiVersion` stays `1.5.0`.
- Tests `pnpm test`, build `pnpm build`, typecheck `pnpm exec tsc --noEmit`.

---

### Task 1: `filterCollection` (pure tree filter)

**Files:**
- Modify: `src/collections.ts` (append)
- Test: `src/collections.test.ts` (append describe block)

**Interfaces:**
- Produces: `filterCollection(c: Collection, query: string): CollectionFilter | null` where `CollectionFilter = { folders: Set<string>; items: Set<string> }`; `null` for an empty/whitespace query. Used by Task 3.

- [ ] **Step 1: Append failing tests** to `src/collections.test.ts` (also add `filterCollection` to the destructured import list at the top):

```ts
describe("filterCollection", () => {
  beforeEach(() => store.clear());

  it("returns null for an empty or whitespace query", async () => {
    const c = await addToCollection("r", blankRequest());
    expect(filterCollection(c, "")).toBeNull();
    expect(filterCollection(c, "   ")).toBeNull();
  });

  it("matches a request by name, url or method and includes ancestor folders", async () => {
    const cf = await createFolder("api");
    const a = cf.folders[0].id;
    const cf2 = await createFolder("users", a);
    const b = cf2.folders[1].id;
    const c = await addToCollection("login", { ...blankRequest(), method: "POST", url: "https://x.io/login" }, b);
    const rid = c.items[0].id;

    for (const q of ["LOGIN", "x.io", "post"]) {
      const vis = filterCollection(c, q)!;
      expect(vis.items.has(rid)).toBe(true);
      expect(vis.folders.has(a)).toBe(true);
      expect(vis.folders.has(b)).toBe(true);
    }
  });

  it("matching a folder name shows its whole subtree and its ancestors", async () => {
    const c1 = await createFolder("top");
    const top = c1.folders[0].id;
    const c2 = await createFolder("api", top);
    const api = c2.folders[1].id;
    const c3 = await createFolder("inner", api);
    const inner = c3.folders[2].id;
    const c = await addToCollection("r", blankRequest(), inner);

    const vis = filterCollection(c, "api")!;
    expect(vis.folders.has(top)).toBe(true);   // ancestor
    expect(vis.folders.has(api)).toBe(true);   // match
    expect(vis.folders.has(inner)).toBe(true); // subtree
    expect(vis.items.has(c.items[0].id)).toBe(true); // request inside subtree
  });

  it("returns empty sets when nothing matches", async () => {
    const c = await addToCollection("r", blankRequest());
    const vis = filterCollection(c, "zzz-nope")!;
    expect(vis.items.size).toBe(0);
    expect(vis.folders.size).toBe(0);
  });
});
```

- [ ] **Step 2: Run** `pnpm exec vitest run src/collections.test.ts` — FAIL (`filterCollection` not exported).

- [ ] **Step 3: Append to `src/collections.ts`:**

```ts
export interface CollectionFilter {
  folders: Set<string>;
  items: Set<string>;
}

/** Visible ids for a search query; null = no filtering (empty query).
 *  Request match (name/url/method) shows the request + ancestor folders;
 *  folder-name match shows the folder, its ancestors and its whole subtree. */
export function filterCollection(c: Collection, query: string): CollectionFilter | null {
  const q = query.trim().toLowerCase();
  if (!q) return null;
  const hit = (s: string) => s.toLowerCase().includes(q);

  const folders = new Set<string>();
  const items = new Set<string>();

  const addAncestors = (folderId: string | null) => {
    let cur = folderId;
    while (cur && !folders.has(cur)) {
      folders.add(cur);
      cur = c.folders.find((f) => f.id === cur)?.parentId ?? null;
    }
  };

  for (const f of c.folders) {
    if (hit(f.name)) {
      addAncestors(f.id);
      for (const id of folderDescendants(c, f.id)) folders.add(id);
    }
  }
  for (const r of c.items) {
    if (
      hit(r.name) || hit(r.request.url) || hit(r.request.method) ||
      (r.folderId !== null && folders.has(r.folderId))
    ) {
      items.add(r.id);
      addAncestors(r.folderId);
    }
  }
  return { folders, items };
}
```

- [ ] **Step 4: Run** `pnpm exec vitest run src/collections.test.ts` — PASS.

- [ ] **Step 5: Commit** — `git add src/collections.ts src/collections.test.ts && git commit -m "feat(collections): filterCollection — search over the folder tree"`

---

### Task 2: `history.ts` (persistent per-project history)

**Files:**
- Create: `src/history.ts`
- Test: `src/history.test.ts`

**Interfaces:**
- Consumes: `ClientRequest` from `./model`, `H.storage`, `H.projects.active()`.
- Produces (used by Tasks 3–4):

```ts
export interface HistoryEntry { ts: number; method: string; url: string; status: number; req: ClientRequest }
export async function loadHistory(): Promise<HistoryEntry[]>
export async function pushHistory(e: HistoryEntry): Promise<HistoryEntry[]>  // prepend, strip file bytes, cap 5000, notify
export async function clearHistory(): Promise<void>                          // persist [] + notify
export function onHistoryChange(cb: (h: HistoryEntry[]) => void): () => void
```

- [ ] **Step 1: Write failing tests** — `src/history.test.ts`:

```ts
import { describe, it, expect, beforeEach } from "vitest";
import { blankRequest } from "./model";

// history.ts reads window.__TRAWL__ at module load — stub it before import.
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

const { loadHistory, pushHistory, clearHistory, onHistoryChange } = await import("./history");

const KEY = "httpclient.history.p1";
const entry = (n: number) => ({
  ts: n, method: "GET", url: `https://a.io/${n}`, status: 200, req: blankRequest(),
});

beforeEach(() => store.clear());

describe("history", () => {
  it("uses a per-project key and prepends entries", async () => {
    await pushHistory(entry(1));
    const h = await pushHistory(entry(2));
    expect(store.has(KEY)).toBe(true);
    expect(h.map((e) => e.ts)).toEqual([2, 1]);
    expect(await loadHistory()).toHaveLength(2);
  });

  it("caps at 5000 entries", async () => {
    const full = Array.from({ length: 5000 }, (_, i) => entry(i));
    store.set(KEY, JSON.stringify(full));
    const h = await pushHistory(entry(9999));
    expect(h).toHaveLength(5000);
    expect(h[0].ts).toBe(9999);
  });

  it("strips multipart file bytes and stores no response fields", async () => {
    const req = {
      ...blankRequest(),
      multipartFiles: [{ key: "f", enabled: true, fileName: "a.bin", fileB64: "AAAA", contentType: "x" }],
    };
    const h = await pushHistory({ ...entry(1), req });
    expect(h[0].req.multipartFiles[0].fileB64).toBe("");
    expect(Object.keys(h[0]).sort()).toEqual(["method", "req", "status", "ts", "url"]);
  });

  it("notifies subscribers and supports unsubscribe; clear empties", async () => {
    const seen: number[] = [];
    const off = onHistoryChange((h) => seen.push(h.length));
    await pushHistory(entry(1));
    await clearHistory();
    off();
    await pushHistory(entry(2));
    expect(seen).toEqual([1, 0]);
    expect((await loadHistory()).map((e) => e.ts)).toEqual([2]);
  });

  it("returns [] for missing or corrupt data", async () => {
    expect(await loadHistory()).toEqual([]);
    store.set(KEY, "not json");
    expect(await loadHistory()).toEqual([]);
  });
});
```

- [ ] **Step 2: Run** `pnpm exec vitest run src/history.test.ts` — FAIL (module missing).

- [ ] **Step 3: Implement `src/history.ts`:**

```ts
// Persistent send history (per project). Response bodies are never stored —
// each entry keeps only the status and a request snapshot for re-opening.

import type { ClientRequest } from "./model";

const H = window.__TRAWL__!;
const MAX_ENTRIES = 5000;

export interface HistoryEntry {
  ts: number;
  method: string;
  /** Final URL (after env substitution). */
  url: string;
  /** 0 on transport errors. */
  status: number;
  /** Editor snapshot; multipart file bytes stripped. */
  req: ClientRequest;
}

function key(): string {
  const p = H.projects.active();
  return `httpclient.history.${p ? p.id : "_global"}`;
}

const listeners = new Set<(h: HistoryEntry[]) => void>();

export function onHistoryChange(cb: (h: HistoryEntry[]) => void): () => void {
  listeners.add(cb);
  return () => void listeners.delete(cb);
}

export async function loadHistory(): Promise<HistoryEntry[]> {
  try {
    const raw = await H.storage.get(key());
    const parsed = raw ? (JSON.parse(raw) as HistoryEntry[]) : [];
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

async function persist(h: HistoryEntry[]): Promise<void> {
  await H.storage.set(key(), JSON.stringify(h));
  listeners.forEach((l) => l(h));
}

export async function pushHistory(e: HistoryEntry): Promise<HistoryEntry[]> {
  const clean: HistoryEntry = {
    ts: e.ts,
    method: e.method,
    url: e.url,
    status: e.status,
    req: { ...e.req, multipartFiles: e.req.multipartFiles.map((f) => ({ ...f, fileB64: "" })) },
  };
  const next = [clean, ...(await loadHistory())].slice(0, MAX_ENTRIES);
  await persist(next);
  return next;
}

export async function clearHistory(): Promise<void> {
  await persist([]);
}
```

- [ ] **Step 4: Run** `pnpm exec vitest run src/history.test.ts` — PASS.

- [ ] **Step 5: Commit** — `git add src/history.ts src/history.test.ts && git commit -m "feat(history): persistent per-project history — cap 5000, no response bodies"`

---

### Task 3: UI — tabs, search, resize, history wiring

**Files:**
- Modify: `src/CollectionTree.tsx` (filter prop)
- Modify: `src/HttpClientApp.tsx` (left panel rebuild)

**Interfaces:**
- Consumes: `filterCollection`/`CollectionFilter` (Task 1), `history.ts` API (Task 2).
- Produces: `CollectionTree` accepts optional `filter?: string`; `HttpClientApp` no longer keeps in-memory history state shape of its own (uses `HistoryEntry` from `./history`; the local `HistoryEntry` interface is deleted).

- [ ] **Step 1: CollectionTree filter.** Add `filterCollection` to its collections import; add `filter` prop; compute `const vis = filterCollection(collection, filter ?? "");`. Change `renderChildren` to skip hidden nodes and force-open folders while filtering:

```tsx
  const renderChildren = (parentId: string | null, depth: number) => (
    <>
      {collection.folders
        .filter((f) => f.parentId === parentId && (!vis || vis.folders.has(f.id)))
        .map((f) => renderFolder(f, depth))}
      {collection.items
        .filter((r) => r.folderId === parentId && (!vis || vis.items.has(r.id)))
        .map((r) => renderRequest(r, depth))}
    </>
  );
```

In `renderFolder`, replace `{!closed.has(f.id) && renderChildren(...)}` with `{(vis !== null || !closed.has(f.id)) && renderChildren(f.id, depth + 1)}`. Replace the final empty-state return with:

```tsx
  const nothingVisible = vis
    ? vis.folders.size === 0 && vis.items.size === 0
    : collection.folders.length === 0 && collection.items.length === 0;
  if (nothingVisible) {
    return <div style={{ fontSize: 11, color: "var(--muted-foreground)", padding: "2px 0" }}>
      {vis ? "No matches." : ""}
    </div>;
  }
  return <div>{renderChildren(null, 0)}</div>;
```

- [ ] **Step 2: HttpClientApp — imports & state.** Import `{ clearHistory, loadHistory, onHistoryChange, pushHistory, type HistoryEntry } from "./history"`; delete the local `HistoryEntry` interface. Add state/refs:

```ts
  type PanelTab = "collection" | "history" | "env";
  const [panelTab, setPanelTabState] = useState<PanelTab>("collection");
  const [panelWidth, setPanelWidth] = useState(250);
  const widthRef = useRef(250);
  const leftRef = useRef<HTMLDivElement>(null);
  const resizing = useRef(false);
  const [collSearch, setCollSearch] = useState("");
  const [histSearch, setHistSearch] = useState("");
  const [clearArmed, setClearArmed] = useState(false);
```

(`history` state stays but is now loaded from the module.) Keep `widthRef.current = panelWidth;` on every render (line right after the state block).

- [ ] **Step 3: Effects.** In the main mount effect add history wiring; add pref restore; project change reloads history:

```ts
    void loadHistory().then(setHistory);
    const offHist = onHistoryChange(setHistory);
    // inside H.projects.onChange callback add:
    void loadHistory().then(setHistory);
    // cleanup: offHist();
```

```ts
  // Restore panel prefs.
  useEffect(() => {
    void H.storage.get("httpclient.panelWidth").then((v) => {
      const w = Number(v);
      if (w >= 180 && w <= 520) setPanelWidth(w);
    });
    void H.storage.get("httpclient.panelTab").then((v) => {
      if (v === "collection" || v === "history" || v === "env") setPanelTabState(v);
    });
  }, []);

  const setPanelTab = (t: PanelTab) => {
    setPanelTabState(t);
    void H.storage.set("httpclient.panelTab", t);
  };
```

- [ ] **Step 4: send() writes through the module.** Replace the `setHistory(...)` call in the success path with:

```ts
      void pushHistory({ ts: Date.now(), method: req.method, url: wire.url, status: r.status, req: snapshot });
```

and in the `catch` block (transport error, status 0) add before/after `patchTab`:

```ts
      const snapshot = clone({ ...req, multipartFiles: req.multipartFiles.map((f) => ({ ...f, fileB64: "" })) });
      void pushHistory({ ts: Date.now(), method: req.method, url: req.url, status: 0, req: snapshot });
```

(move the existing `snapshot` computation above the `try` so both paths share it; in the catch use `req.url` since the wire URL may not exist).

- [ ] **Step 5: Resize handlers + left column JSX.** Handlers:

```ts
  const onResizeDown = (e: React.PointerEvent<HTMLDivElement>) => {
    resizing.current = true;
    e.currentTarget.setPointerCapture(e.pointerId);
  };
  const onResizeMove = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!resizing.current) return;
    const left = leftRef.current?.getBoundingClientRect().left ?? 0;
    setPanelWidth(Math.min(520, Math.max(180, Math.round(e.clientX - left))));
  };
  const onResizeUp = () => {
    if (!resizing.current) return;
    resizing.current = false;
    void H.storage.set("httpclient.panelWidth", String(widthRef.current));
  };
```

History filtering (before `return`):

```ts
  const histQ = histSearch.trim().toLowerCase();
  const histFiltered = histQ
    ? history.filter(
        (h) =>
          h.url.toLowerCase().includes(histQ) ||
          h.method.toLowerCase().includes(histQ) ||
          String(h.status).includes(histQ),
      )
    : history;
```

Replace the whole left column (`<div style={styles.left}>…</div>` with its three `Section`s) by:

```tsx
      <div ref={leftRef} style={{ ...styles.left, width: panelWidth }}>
        <Tabs
          value={panelTab}
          onChange={(v) => setPanelTab(v as PanelTab)}
          tabs={[
            { value: "collection", label: "Collection" },
            { value: "history", label: "History" },
            { value: "env", label: "Env" },
          ]}
        />
        <div style={{ paddingTop: 10 }}>
          {panelTab === "collection" && (
            <>
              <div style={styles.panelHead}>
                <input
                  style={{ ...styles.cell, flex: 1 }}
                  value={collSearch}
                  placeholder="Search…"
                  onChange={(e) => setCollSearch(e.target.value)}
                />
                <button style={styles.sectionAction} onClick={() => setNewFolderName("")}>+ Folder</button>
              </div>
              {newFolderName !== null && (
                /* existing folder-name input row, unchanged */
              )}
              {saveName !== null && (
                /* existing save flow (name input + folder select + Save), unchanged */
              )}
              <CollectionTree collection={collection} onOpen={openSaved} onChanged={setCollection} filter={collSearch} />
            </>
          )}
          {panelTab === "history" && (
            <>
              <div style={styles.panelHead}>
                <input
                  style={{ ...styles.cell, flex: 1 }}
                  value={histSearch}
                  placeholder="Search…"
                  onChange={(e) => setHistSearch(e.target.value)}
                />
                <button
                  style={{ ...styles.sectionAction, ...(clearArmed ? { color: "var(--http-red)" } : {}) }}
                  onClick={() => {
                    if (!clearArmed) {
                      setClearArmed(true);
                      return;
                    }
                    setClearArmed(false);
                    void clearHistory();
                  }}
                  onBlur={() => setClearArmed(false)}
                >
                  {clearArmed ? "Sure?" : "Clear"}
                </button>
              </div>
              {histFiltered.length === 0 && <Muted>No history.</Muted>}
              {histFiltered.slice(0, 200).map((h, i) => (
                <button
                  key={`${h.ts}-${i}`}
                  style={styles.histRow}
                  title={`${new Date(h.ts).toLocaleString()} — open in a new tab`}
                  onClick={() => openHistory(h)}
                >
                  <MethodBadge method={h.method} />
                  <span style={styles.histUrl}>{h.url}</span>
                  <span style={styles.histStatus}>{h.status || "—"}</span>
                </button>
              ))}
              {histFiltered.length > 200 && <Muted>showing 200 of {histFiltered.length}</Muted>}
            </>
          )}
          {panelTab === "env" && (
            <>
              {!project ? (
                <Muted>Select a project to use variables.</Muted>
              ) : (
                <>
                  <div style={styles.panelHead}>
                    <Muted>Environment · {project.name}</Muted>
                    <button style={styles.sectionAction} onClick={() => void saveEnv()}>
                      {envDirty ? "Save*" : "Save"}
                    </button>
                  </div>
                  <RowList
                    rows={env.map((e) => ({ key: e.key, value: e.value, enabled: true }))}
                    onChange={(rows) => { setEnv(rows.map((r) => ({ key: r.key, value: r.value }))); setEnvDirty(true); }}
                    addLabel="+ Add variable"
                    noToggle
                  />
                  <Muted>Use as {"{{name}}"} in URL, headers, params, body. Shared with capture & scripts.</Muted>
                </>
              )}
            </>
          )}
        </div>
      </div>
      <div
        style={styles.resizer}
        title="Drag to resize"
        onPointerDown={onResizeDown}
        onPointerMove={onResizeMove}
        onPointerUp={onResizeUp}
      />
```

The `CollectionTree` render is no longer wrapped in the old empty-check (the tree itself shows "No matches."/empty). Delete the now-unused `Section` helper and the `section`, `sectionHead`, `sectionTitle` styles (keep `sectionAction` — still used). Style changes:

```ts
  left: { flexShrink: 0, borderRight: "1px solid var(--border)", background: "var(--card)", overflow: "auto", padding: 12 },  // width removed (dynamic)
  resizer: { width: 5, flexShrink: 0, cursor: "col-resize", background: "transparent" },
  panelHead: { display: "flex", alignItems: "center", gap: 6, marginBottom: 8 },
```

- [ ] **Step 6: Verify** — `pnpm exec tsc --noEmit` clean; `pnpm exec vitest run` all PASS.

- [ ] **Step 7: Commit** — `git add src/CollectionTree.tsx src/HttpClientApp.tsx && git commit -m "feat(ui): panel tabs + search + drag resize; history via persistent module"`

---

### Task 4: MCP `send_request` writes history

**Files:**
- Modify: `src/mcpTools.ts` (send_request handler)
- Test: `src/mcpTools.test.ts` (append test; add `httpclient.history.p1` check)

**Interfaces:**
- Consumes: `pushHistory` from `./history` (Task 2).

- [ ] **Step 1: Append failing test** to the `send_request` describe block in `src/mcpTools.test.ts`:

```ts
  it("records the send into persistent history (no response body)", async () => {
    await call("send_request", { url: "https://{{host}}/v1" });
    const raw = store.get("httpclient.history.p1");
    expect(raw).toBeTruthy();
    const h = JSON.parse(raw!) as { url: string; status: number; req: unknown; body?: unknown }[];
    expect(h).toHaveLength(1);
    expect(h[0].url).toBe("https://api.io/v1");
    expect(h[0].status).toBe(200);
    expect(h[0].body).toBeUndefined();
  });
```

- [ ] **Step 2: Run** `pnpm exec vitest run src/mcpTools.test.ts` — FAIL (no history written).

- [ ] **Step 3: Implement.** In `src/mcpTools.ts` add `import { pushHistory } from "./history";` and rework the `send_request` handler body tail:

```ts
      const env = a.substituteEnv === false ? [] : host.projects.active()?.env ?? [];
      const wire = toSendRequest(req, env);
      const res = await host.http.send(wire, a.viaProxy === true);
      publishResponse(res);
      void pushHistory({ ts: Date.now(), method: req.method, url: wire.url, status: res.status, req });
      return truncateResponse(res);
```

- [ ] **Step 4: Run** `pnpm exec vitest run` — all PASS.

- [ ] **Step 5: Commit** — `git add src/mcpTools.ts src/mcpTools.test.ts && git commit -m "feat(mcp): send_request records into persistent history"`

---

### Task 5: Version 0.6.0, README, build

**Files:**
- Modify: `package.json`, `trawl-plugin.json` (version → `0.6.0`)
- Modify: `README.md`
- Modify: `dist/plugin.js` (rebuild, committed)

- [ ] **Step 1: Bump** `"version": "0.5.0"` → `"0.6.0"` in both json files (`apiVersion` stays `1.5.0`).

- [ ] **Step 2: README.** In the «Collections & folders» section append:

```markdown
The left panel is resizable (drag its right edge) and split into
**Collection | History | Env** tabs, each list with instant search. Send
history persists per project (last 5000 sends, request snapshots only — no
response bodies) and survives restarts; **Clear** wipes it.
```

- [ ] **Step 3: Verify + build** — `pnpm exec tsc --noEmit && pnpm test && pnpm build` (all green, `dist/plugin.js` rebuilt).

- [ ] **Step 4: Commit** — `git add package.json trawl-plugin.json README.md dist/ && git commit -m "chore: v0.6.0 — panel tabs, search, resize, persistent history"`

- [ ] **Step 5: Finish** — merge `feature/v0.6` to main (ff), re-run `pnpm test` on main, remove worktree, delete branch, `git push origin main v0.6.0` with annotated tag `v0.6.0` («v0.6.0 — panel tabs, search, resize, persistent history»).
