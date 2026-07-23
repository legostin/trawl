import * as React from "react";
import * as JsxRuntime from "react/jsx-runtime";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  aggregateFlows,
  deleteReport,
  flowCount,
  listReports,
  queryFlows,
  saveReport,
  type FlowQuery,
} from "@/db";
import { invoke } from "@tauri-apps/api/core";
import { useFlows } from "@/store";
import { useProjects } from "@/projects";
import { useRules } from "@/rules";
import { useLayout } from "@/layout";
import { usePlugins } from "@/plugins";
import { sendRequest } from "@/http";
import { bodyToText } from "@/lib/body";
import { buildCurl } from "@/lib/curl";
import { BodyViewer } from "@/components/BodyViewer";
import { HeadersTable } from "@/components/HeadersTable";
import { MethodBadge, StatusBadge } from "@/components/badges";
import { ScriptEditor } from "@/components/ScriptEditor";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { analyzeJson, fieldsToType } from "@/lib/analyze";
import { setEventPayloadType } from "@/monaco-setup";
import { listSecrets, getSecret, setSecret, deleteSecret } from "@/secrets";
import { useUpdater } from "@/updater";
import { bus } from "./bus";
import { initMcpBridge, registerTool, unregisterTool } from "./mcpBridge";
import {
  HOST_API_VERSION,
  type ActiveProject,
  type EnvVar,
  type EventParam,
  type FlowAction,
  type RegisteredMode,
  type RuleDraft,
  type TrawlHost,
  type TrawlUi,
} from "./api";

/** Snapshot the active project (id/name/env) from the projects store. */
function activeProject(): ActiveProject | null {
  const { projects, activeId } = useProjects.getState();
  const p = projects.find((x) => x.id === activeId);
  return p ? { id: p.id, name: p.name, env: p.env } : null;
}

/** Scope a flow query to the active project (matching capture behaviour), unless
 *  the caller set `projectId` explicitly. Keeps plugin data consistent with the
 *  traffic list and the active-project selector. */
function scoped(f: FlowQuery): FlowQuery {
  const activeId = useProjects.getState().activeId;
  return activeId && f.projectId === undefined ? { ...f, projectId: activeId } : f;
}

let installed = false;

/** Documented payload fields shared by every event carrying a Flow object
 *  (capture + breakpoint lifecycle). */
const FLOW_PARAMS: EventParam[] = [
  { name: "id", type: "number", doc: "Flow id" },
  { name: "timestamp", type: "number", doc: "Capture time, ms since epoch" },
  { name: "method", type: "string", doc: "HTTP method" },
  { name: "url.host", type: "string", doc: "Request host" },
  { name: "url.path", type: "string", doc: "Request path" },
  {
    name: "state",
    type: "string",
    doc: '"pending" | "completed" | "error" | "paused"',
  },
  { name: "error", type: "string | null", doc: 'Error message, set when state is "error"' },
  { name: "appliedRules", type: "string[]", doc: "Names of rules applied to this flow" },
  {
    name: "response.status",
    type: "number | undefined",
    doc: "Response status code, once a response has arrived",
  },
  {
    name: "pausedPhase",
    type: '"request" | "response" | null',
    doc: "Phase the flow is/was paused in, while held on a breakpoint",
  },
  {
    name: "timings.sent",
    type: "number | null",
    doc: "Ms offset the request was sent, null until known",
  },
  {
    name: "timings.done",
    type: "number | null",
    doc: "Ms offset the flow completed, null until known",
  },
];

/** Documented payload fields shared by rule outcome events. */
const RULE_PARAMS: EventParam[] = [
  { name: "ruleName", type: "string", doc: "Name of the rule that ran" },
  { name: "phase", type: "string", doc: '"request" | "response" | "handler"' },
  { name: "flowId", type: "number", doc: "Id of the flow the rule acted on" },
  { name: "method", type: "string", doc: "HTTP method of the flow" },
  { name: "host", type: "string", doc: "Request host" },
  { name: "path", type: "string", doc: "Request path" },
];
const RULE_ERROR_PARAMS: EventParam[] = [
  ...RULE_PARAMS,
  { name: "error", type: "string", doc: "Script error message" },
];

/** Flow ids already reported via flow:error, FIFO-capped so long sessions don't
 *  grow this set unboundedly. */
const erroredFlowIds = new Set<number>();
const erroredFlowOrder: number[] = [];

/** Records that `id` has been reported; returns false if it was already seen. */
function markFlowErrored(id: number): boolean {
  if (erroredFlowIds.has(id)) return false;
  erroredFlowIds.add(id);
  erroredFlowOrder.push(id);
  if (erroredFlowOrder.length > 1000) {
    const oldest = erroredFlowOrder.shift();
    if (oldest !== undefined) erroredFlowIds.delete(oldest);
  }
  return true;
}

/** Derive `flow:error` from a flow-added/flow-updated payload (once per flow id). */
function maybeEmitFlowError(payload: unknown): void {
  const p = payload as { id?: number; state?: string } | null | undefined;
  if (p && typeof p.id === "number" && p.state === "error" && markFlowErrored(p.id)) {
    bus.emit("flow:error", payload);
  }
}

/** Install the host API on `window` and bridge app state into the event bus. */
export function installHost(): void {
  if (installed) return;
  installed = true;

  const host: TrawlHost = {
    version: HOST_API_VERSION,
    react: React,
    events: {
      on: (t, cb) => bus.on(t, cb),
      off: (t, cb) => bus.off(t, cb),
      emit: (t, p) => bus.emit(t, p),
      describe: (t, m) => bus.describe(t, m),
      known: () => bus.known(),
    },
    flows: {
      query: (f, limit, offset) => queryFlows(scoped(f), limit, offset),
      count: (f) => flowCount(scoped(f)),
      aggregate: (f, g, bucket, limit) => aggregateFlows(scoped(f), g, bucket, limit),
      subscribe: (cb) => {
        const u1 = bus.on("flow:added", cb);
        const u2 = bus.on("flow:updated", cb);
        return () => {
          u1();
          u2();
        };
      },
    },
    reports: {
      save: (r) => saveReport(r),
      list: () => listReports(),
      remove: (id) => deleteReport(id),
    },
    http: {
      // viaProxy must work even when the proxy is stopped: start it on demand
      // so the request is captured and the topbar reflects the running proxy.
      send: async (req, viaProxy) => {
        if (viaProxy) await useFlows.getState().ensureProxy();
        return sendRequest(req, viaProxy);
      },
    },
    projects: {
      active: () => activeProject(),
      setEnv: async (env: EnvVar[]) => {
        const { projects, activeId, upsert } = useProjects.getState();
        const p = projects.find((x) => x.id === activeId);
        if (!p) return;
        await upsert({ ...p, env });
      },
      onChange: (cb: (project: ActiveProject | null) => void) =>
        bus.on("project:changed", () => cb(activeProject())),
    },
    rules: {
      create: async (draft: RuleDraft) => {
        await useRules.getState().upsert({
          id: crypto.randomUUID(),
          enabled: true,
          projectId: useProjects.getState().activeId ?? null,
          ...draft,
        });
        // Land the user in the rules editor with the new rule selected.
        useLayout.getState().setMode("traffic");
        useFlows.getState().setView("rules");
      },
    },
    gitHosts: {
      token: (host: string) => invoke<string | null>("git_host_token_get", { host }),
      hasToken: (host: string) => invoke<boolean>("git_host_token_has", { host }),
      setToken: (host: string, token: string) =>
        invoke<void>("git_host_token_set", { host, token }),
    },
    storage: {
      get: (key: string) => invoke<string | null>("plugin_storage_get", { key }),
      set: (key: string, value: string) =>
        invoke<void>("plugin_storage_set", { key, value }),
    },
    secrets: {
      list: () => listSecrets(),
      get: (name: string) => getSecret(name),
      set: (name: string, value: string) => setSecret(name, value),
      remove: (name: string) => deleteSecret(name),
    },
    mcp: { registerTool, unregisterTool },
    ui: {
      BodyViewer,
      HeadersTable,
      MethodBadge,
      StatusBadge,
      ScriptEditor,
      // Button's variant/size are cva-generated literal unions; the host API only
      // promises `string` to plugins, so the real component needs a cast here.
      Button: Button as unknown as TrawlUi["Button"],
      Input,
      Select,
    },
    util: {
      bodyText: (msg) => bodyToText(msg),
      buildCurl: (flow) => buildCurl(flow),
      inferTypeBody: (samples: unknown[]) => fieldsToType(analyzeJson(samples)),
      inferFields: (samples: unknown[]) =>
        analyzeJson(samples).map(({ path, type, example }) => ({ path, type, example })),
      setPayloadType: (typeBody: string) => setEventPayloadType(typeBody),
    },
    openUrl: (url: string) => openUrl(url),
    registerMode: (mode: RegisteredMode) => usePlugins.getState().registerMode(mode),
    registerFlowAction: (action: FlowAction) =>
      usePlugins.getState().registerFlowAction(action),
    setMode: (id: string) => useLayout.getState().setMode(id),
    log: (...args) => console.log("[plugin]", ...args),
  };

  window.React = React;
  window.ReactJSXRuntime = JsxRuntime;
  window.__TRAWL__ = host;
  initMcpBridge();

  // Bridge Tauri capture events into the plugin bus.
  void listen("flow-added", (e) => {
    bus.emit("flow:added", e.payload);
    maybeEmitFlowError(e.payload);
  });
  void listen("flow-updated", (e) => {
    bus.emit("flow:updated", e.payload);
    maybeEmitFlowError(e.payload);
  });

  // Breakpoint lifecycle → plugin bus.
  void listen("flow-paused", (e) => bus.emit("breakpoint:hit", e.payload));
  void listen("flow-resumed", (e) => bus.emit("breakpoint:resolved", e.payload));
  void listen("breakpoint-timeout", (e) => bus.emit("breakpoint:timeout", e.payload));

  // Rule outcomes → plugin bus.
  void listen("rule-applied", (e) => bus.emit("rule:applied", e.payload));
  void listen("rule-error", (e) => bus.emit("rule:error", e.payload));

  // Script notify() → plugin bus (delivery is a plugin concern, e.g. Telegram).
  void listen("script-notify", (e) => bus.emit("notify:send", e.payload));

  const FLOW_TYPE = `{
    id: number; timestamp: number; method: string;
    url: { scheme: string; host: string; port: number; path: string };
    request: { headers: [string, string][]; body: number[] | string; bodyIsText: boolean };
    response: { status: number; headers: [string, string][]; body: number[] | string; bodyIsText: boolean } | null;
    state: string; error: string | null; appliedRules: string[];
    timings: { sent: number | null; ttfb: number | null; done: number | null };
    pausedPhase?: "request" | "response" | null;
  }`;
  const RULE_TYPE =
    "{ ruleName: string; phase: string; flowId: number; method: string; host: string; path: string }";
  const RULE_ERROR_TYPE =
    "{ ruleName: string; phase: string; flowId: number; method: string; host: string; path: string; error: string }";

  bus.describe("flow:added", {
    description: "A new request/response was captured",
    payloadType: FLOW_TYPE,
    params: FLOW_PARAMS,
    source: "core",
  });
  bus.describe("flow:updated", {
    description: "A captured flow changed (response arrived, breakpoint resolved)",
    payloadType: FLOW_TYPE,
    params: FLOW_PARAMS,
    source: "core",
  });
  bus.describe("flow:error", {
    description: 'A captured flow reached the "error" state (proxy or script failure)',
    payloadType: FLOW_TYPE,
    params: FLOW_PARAMS,
    source: "core",
  });
  bus.describe("capture:started", { description: "The proxy started", source: "core" });
  bus.describe("capture:stopped", { description: "The proxy stopped", source: "core" });
  bus.describe("filter:changed", {
    description: "The traffic search/filter changed",
    payloadType: "{ [key: string]: any }",
    params: [
      { name: "query", type: "string", doc: "Free-text search over host + path" },
      { name: "method", type: "string", doc: 'HTTP method filter, "" for any' },
      { name: "statusClass", type: "string", doc: '"any" | "2xx" | "3xx" | "4xx" | "5xx"' },
    ],
    source: "core",
  });
  bus.describe("project:changed", {
    description: "The active project selector changed",
    payloadType: "string | null",
    params: [
      {
        name: "projectId",
        type: "string | null",
        doc: "New active project id, or null when capturing all domains",
      },
    ],
    source: "core",
  });
  bus.describe("notify:send", {
    description: "Deliver a notification (emitted by rule notify() and plugins)",
    payloadType:
      "{ text: string; channel?: string; title?: string; source?: string; ruleName?: string; flowId?: number }",
    params: [
      { name: "text", type: "string", doc: "Notification body" },
      { name: "channel", type: "string", doc: "Delivery channel hint, e.g. a plugin id" },
      { name: "title", type: "string", doc: "Optional notification title" },
      { name: "source", type: "string", doc: 'Origin, e.g. "rule" or a plugin id' },
      { name: "ruleName", type: "string", doc: "Rule that triggered notify(), if any" },
      { name: "flowId", type: "number", doc: "Related flow id, if any" },
    ],
    source: "core",
  });
  bus.describe("breakpoint:hit", {
    description: "A flow paused at a breakpoint, awaiting resolution",
    payloadType: FLOW_TYPE,
    params: FLOW_PARAMS,
    source: "core",
  });
  bus.describe("breakpoint:resolved", {
    description: "A paused flow was resumed (edited or passed through)",
    payloadType: FLOW_TYPE,
    params: FLOW_PARAMS,
    source: "core",
  });
  bus.describe("breakpoint:timeout", {
    description: "A paused flow auto-continued after the breakpoint timeout elapsed",
    payloadType: FLOW_TYPE,
    params: FLOW_PARAMS,
    source: "core",
  });
  bus.describe("rule:applied", {
    description: "A rule ran successfully against a flow",
    payloadType: RULE_TYPE,
    params: RULE_PARAMS,
    source: "core",
  });
  bus.describe("rule:error", {
    description: "A rule's script threw while running against a flow",
    payloadType: RULE_ERROR_TYPE,
    params: RULE_ERROR_PARAMS,
    source: "core",
  });
  bus.describe("plugin:installed", {
    description: "A plugin finished installing",
    payloadType: "{ id: string; name: string; version: string }",
    params: [
      { name: "id", type: "string", doc: "Plugin id" },
      { name: "name", type: "string", doc: "Plugin display name" },
      { name: "version", type: "string", doc: "Installed version" },
    ],
    source: "core",
  });
  bus.describe("plugin:removed", {
    description: "A plugin was uninstalled",
    payloadType: "{ id: string }",
    params: [{ name: "id", type: "string", doc: "Removed plugin id" }],
    source: "core",
  });
  bus.describe("update:available", {
    description: "A newer app version is available",
    payloadType: "{ version: string; notes: string | null }",
    params: [
      { name: "version", type: "string", doc: "Newer version's number" },
      { name: "notes", type: "string | null", doc: "Release notes, if provided" },
    ],
    source: "core",
  });

  // Bridge relevant store changes into the bus (bidirectional: plugins can also emit).
  let lastFilter = useFlows.getState().filter;
  let lastRunning = useFlows.getState().running;
  useFlows.subscribe((s) => {
    if (s.filter !== lastFilter) {
      lastFilter = s.filter;
      bus.emit("filter:changed", s.filter);
    }
    if (s.running !== lastRunning) {
      lastRunning = s.running;
      bus.emit(s.running ? "capture:started" : "capture:stopped");
    }
  });

  let lastProject = useProjects.getState().activeId;
  useProjects.subscribe((s) => {
    if (s.activeId !== lastProject) {
      lastProject = s.activeId;
      bus.emit("project:changed", s.activeId);
    }
  });

  // Dedupe by version (not just status transition) — a status can flip away
  // from "available" and back (or re-notify) without a new version ever
  // showing up, and we only want to tell plugins about it once.
  let lastEmittedUpdateVersion: string | null = null;
  useUpdater.subscribe((s) => {
    if (s.status === "available" && s.version !== lastEmittedUpdateVersion) {
      lastEmittedUpdateVersion = s.version;
      bus.emit("update:available", { version: s.version, notes: s.notes });
    }
  });
}
