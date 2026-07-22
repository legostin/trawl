# Writing Trawl plugins

A Trawl plugin is a small web app that runs **inside the Trawl UI**. It can:

- add a whole new **mode** (a top-level view with its own sidebar entry),
- add **action buttons** to the request-detail toolbar,
- read live and historical **traffic**, send HTTP requests, create rules,
  read/write project env vars, access app-wide **Keychain secrets**, send
  notifications, persist its own data, and talk to other plugins over an
  **event bus**,
- reuse Trawl’s own UI components so it looks native.

Plugins are distributed as a **GitHub repository** containing a manifest and a
built JS bundle. Users install them by URL from the **Plugins** tab.

> **Trust model.** Plugins run with full access to the host app (they share its
> React instance and `window`). Only install plugins you trust.

---

## The model

- A plugin ships a single **IIFE bundle** (`dist/plugin.js`) built with
  `react`, `react-dom`, and `react/jsx-runtime` marked **external** — the host
  provides those as globals so the plugin shares the host’s one React instance.
- On load, the bundle runs and calls into the host object at
  **`window.__TRAWL__`** (e.g. `registerMode(...)`), self-registering its
  contributions.
- The host injects the bundle as a classic `<script>` at startup for every
  enabled plugin (and immediately after install). Re-loading replaces the
  previous injection, so enable/disable and updates apply live.

---

## Manifest — `trawl-plugin.json`

Placed at the repo root:

```json
{
  "id": "http-client",
  "name": "HTTP Client",
  "version": "0.3.1",
  "description": "Postman-style HTTP client for Trawl.",
  "author": "you",
  "entry": "dist/plugin.js",
  "apiVersion": "1.5.0",
  "dependencies": []
}
```

| Field | Required | Meaning |
| --- | --- | --- |
| `id` | ✓ | Unique, stable plugin id (also the mode id you typically register). |
| `name` | ✓ | Display name. |
| `entry` | ✓ | Path in the repo to the built bundle, e.g. `dist/plugin.js`. |
| `version` | | Dotted-numeric (`0.3.1`). Used for update comparisons. |
| `description`, `author` | | Shown in the Plugins list. |
| `apiVersion` | | Host API version this plugin targets (see [Versioning](#versioning)). |
| `dependencies` | | Other plugins to auto-install (see below). |

### Dependencies

A plugin can require other plugins; Trawl installs them recursively:

```json
"dependencies": [
  {
    "id": "some-lib",
    "repo": "owner/trawl-plugin-some-lib",
    "host": "github.com",
    "reference": "main",
    "minVersion": "1.0.0"
  }
]
```

`host` defaults to `github.com`; `reference` (git branch/tag) defaults to `main`.
`minVersion` reinstalls the dependency if the installed copy is older.

---

## Installing a plugin

**Plugins** tab → paste the repo reference and install. Accepted forms:

- `owner/repo` (github.com), or a full URL,
- GitHub Enterprise: `github.example.org/owner/repo` (or a `.../tree/<ref>` URL).

For private/GHE repos, Trawl asks for a **per-host token** once and stores it;
the token is handed to the backend for fetching (and to plugins that ask for it
via `__TRAWL__.gitHosts`). Trawl downloads `trawl-plugin.json` and the `entry`
bundle over HTTPS and caches them locally.

---

## The host API — `window.__TRAWL__`

Everything is reachable from the global `host`:

```ts
const host = window.__TRAWL__;
```

```ts
interface TrawlHost {
  version: string;                 // host API version, e.g. "1.5.0"
  react: typeof React;             // the host's React (use for hooks if needed)

  events: PluginEvents;            // pub/sub bus (see Events)
  flows: PluginFlows;              // query/aggregate + live subscribe to traffic
  reports: PluginReports;          // save/list/remove saved reports
  http: TrawlHttp;                 // one-shot HTTP send (optionally via the proxy)
  projects: TrawlProjects;         // active project + env vars
  gitHosts: TrawlGitHosts;         // per-host git tokens
  secrets: TrawlSecrets;           // app-wide named secrets (Keychain)
  rules: TrawlRules;               // create a rule and open the editor
  storage: TrawlStorage;           // project-scoped key/value persistence
  ui: TrawlUi;                     // reusable host components
  util: TrawlUtil;                 // bodyText(), buildCurl()

  registerMode(mode): void;        // add a top-level mode + sidebar entry
  registerFlowAction(action): void;// add a button to the request toolbar
  openUrl(url): Promise<void>;     // open in the system browser
  setMode(id): void;               // switch the active top-level mode
  log(...args): void;              // console log, prefixed [plugin]
}
```

### Contributions

```ts
host.registerMode({
  id: "http-client",
  label: "HTTP Client",
  icon: MyIcon,                    // optional: (props:{className?})=>JSX
  component: MyPanel,              // React component rendered when active
});

host.registerFlowAction({
  id: "send-to-client",
  label: "To client",
  icon: MyIcon,
  run: (flow) => { /* ... */ host.setMode("http-client"); },
});
```

### `flows` — traffic

```ts
host.flows.query(filter, limit?, offset?): Promise<FlowRow[]>
host.flows.count(filter): Promise<number>
host.flows.aggregate(filter, groupBy, bucket?, limit?): Promise<AggBucket[]>
host.flows.subscribe(cb): () => void   // fires on every new/updated flow
```

Queries are automatically **scoped to the active project** (matching capture
behaviour) unless the `filter` sets `projectId` explicitly.

### `http` — send a request

```ts
const res = await host.http.send(request, /* viaProxy? */ true);
```

### `projects` — active project & env

```ts
host.projects.active(): { id, name, env: {key,value}[] } | null
host.projects.setEnv(env): Promise<void>
host.projects.onChange(cb): () => void
```

### `rules` — create a rule

```ts
await host.rules.create({
  name: "add auth",
  pattern: "api.example.com/*",
  phase: "request",              // "request" | "response" | "both" | "handler"
  script: "setHeader(request,'authorization','Bearer '+env.token);",
});
```

The rule is created in the active project and the rules editor is opened.

### `storage` — persist plugin data

```ts
await host.storage.set("key", JSON.stringify(value));
const raw = await host.storage.get("key");   // string | null
```

### `gitHosts` — git tokens

```ts
await host.gitHosts.hasToken("github.example.org");   // boolean
await host.gitHosts.token("github.example.org");      // string | null
await host.gitHosts.setToken("github.example.org", t);
```

### `secrets` — app-wide named secrets

Stored in the macOS Keychain, managed in **Setup → Secrets**. Shared with rule
scripts (`secret('NAME')`).

```ts
await host.secrets.list();          // string[]
await host.secrets.get("TG_BOT_TOKEN");   // string | null
await host.secrets.set("TG_BOT_TOKEN", token);
await host.secrets.remove("TG_BOT_TOKEN");
```

### `ui` / `util` — render like the host

```ts
const { BodyViewer, HeadersTable, MethodBadge, StatusBadge } = host.ui;
host.util.bodyText(flow.request);   // decoded body text
host.util.buildCurl(flow);          // cURL string
```

---

## MCP tools

Trawl runs a local MCP server (Setup → MCP server) that AI agents connect to.
A plugin can contribute its own tools:

```ts
const host = window.__TRAWL__;

host.mcp.registerTool({
  name: "flaky_endpoints",              // exposed as "<pluginId>_flaky_endpoints"
  description: "Endpoints with the highest error rate.",
  inputSchema: {
    type: "object",
    properties: { limit: { type: "integer" } },
  },
  async handler({ limit = 10 }) {
    const buckets = await host.flows.aggregate({ statusClass: "5xx" }, "host", 0, limit);
    return { buckets };                 // any JSON-serializable value
  },
  timeoutMs: 30_000,                    // optional, default 60000
});
```

Rules:

- `registerTool` must be called **during plugin initialization** (top level of
  your bundle) — that is how the tool is attributed to your plugin.
- Tool names: `[a-zA-Z0-9_-]`. The final MCP name is `<pluginId>_<name>`.
- The handler's return value is serialized to JSON for the agent; thrown
  errors become tool errors.
- Tools are removed automatically when the plugin is disabled or reloaded.

Requires host API `1.6.0`.

---

## Events

The bus is symmetric: the host emits app events, and plugins can `emit`/`on`
their own (great for plugin↔plugin messaging).

```ts
const off = host.events.on("flow:added", (flow) => { /* ... */ });
host.events.emit("my-plugin:did-thing", payload);
off(); // unsubscribe
```

### Event registry & payload hints

Declare your events so other plugins can subscribe with autocomplete:

```ts
host.events.describe("my-plugin:did-thing", {
  description: "Fired after the thing is done",
  payloadType: "{ id: string; ok: boolean }",   // TS type expression
  source: "my-plugin",
});
host.events.known();  // [{ type, description?, payloadType?, source?, lastPayload? }]
```

The bus also remembers the **last payload** of every event, so undeclared
events still get structure-based hints.

### Host-emitted events

| Event | Payload | When |
| --- | --- | --- |
| `flow:added` | `Flow` | A new request/response was captured. |
| `flow:updated` | `Flow` | A captured flow changed (e.g. response arrived, breakpoint resolved). |
| `capture:started` | — | The proxy started. |
| `capture:stopped` | — | The proxy stopped. |
| `filter:changed` | filter object | The traffic search/filter changed. |
| `project:changed` | active project id \| null | The active project selector changed. |
| `notify:send` | `{ text, channel?, title?, source?, ruleName?, flowId? }` | A rule script called `notify()` (or a plugin asked for a notification). Handled by notification plugins (e.g. Telegram). |

> `host.flows.subscribe(cb)` is a convenience over `flow:added` + `flow:updated`.

Custom event names are free-form strings; namespace them with your plugin id
(`my-plugin:...`) to avoid collisions.

---

## Writing a plugin

### 1. Scaffold

The simplest way is to mirror an existing plugin (e.g. `trawl-plugin-http-client`).
You need: a `trawl-plugin.json`, a Vite build that emits an IIFE with React
externalized, and a `src/plugin.tsx` entry that registers your contributions.

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
      name: "MyTrawlPlugin",
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

### 2. Entry — `src/plugin.tsx`

```tsx
const host = window.__TRAWL__;

function Panel() {
  const [n, setN] = host!.react.useState(0);
  return (
    <div style={{ padding: 16 }}>
      <h2>Hello from a plugin</h2>
      <button onClick={() => setN((x) => x + 1)}>clicked {n}</button>
    </div>
  );
}

if (host) {
  host.registerMode({ id: "hello", label: "Hello", component: Panel });

  host.registerFlowAction({
    id: "hello-log",
    label: "Log flow",
    run: (flow) => host.log("flow", flow.method, flow.url.path),
  });

  // React to captured traffic:
  host.flows.subscribe((flow) => host.log("captured", flow));
}
```

Use `host.react` for hooks so you share the host’s React instance. Styling can
use the host’s Tailwind classes (e.g. `text-muted-foreground`, `border-border`)
to match the app.

### 3. Types (optional but recommended)

Add an ambient `src/trawl.d.ts` declaring the subset of `window.__TRAWL__` you
use — mirror [`src/plugins/api.ts`](../src/plugins/api.ts) in the host repo,
which is the source of truth.

### 4. Build & publish

```sh
pnpm install
pnpm build            # emits dist/plugin.js
git add -A && git commit -m "build" && git push
```

Commit the built `dist/plugin.js` — Trawl fetches the `entry` file directly from
the repo. Then install it in Trawl’s **Plugins** tab by its `owner/repo`.

---

## Versioning

`host.version` (and the `HOST_VERSION` in the host) is the API version. Put the
version your plugin targets in the manifest’s `apiVersion`. The API is additive;
newer host versions keep older plugins working. Check `host.version` at runtime
if you want to feature-detect newer capabilities.

Reference: the full, authoritative API surface lives in the host repo at
[`src/plugins/api.ts`](../src/plugins/api.ts) and its implementation at
[`src/plugins/host.ts`](../src/plugins/host.ts).
