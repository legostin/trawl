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
import { analyzeJson, fieldsToType } from "@/lib/analyze";
import { setEventPayloadType } from "@/monaco-setup";
import { listSecrets, getSecret, setSecret, deleteSecret } from "@/secrets";
import { bus } from "./bus";
import type {
  ActiveProject,
  EnvVar,
  FlowAction,
  RegisteredMode,
  RuleDraft,
  TrawlHost,
} from "./api";

const HOST_VERSION = "1.6.0";

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

/** Install the host API on `window` and bridge app state into the event bus. */
export function installHost(): void {
  if (installed) return;
  installed = true;

  const host: TrawlHost = {
    version: HOST_VERSION,
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
      send: (req, viaProxy) => sendRequest(req, viaProxy),
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
    ui: { BodyViewer, HeadersTable, MethodBadge, StatusBadge, ScriptEditor },
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

  // Bridge Tauri capture events into the plugin bus.
  void listen("flow-added", (e) => bus.emit("flow:added", e.payload));
  void listen("flow-updated", (e) => bus.emit("flow:updated", e.payload));

  // Script notify() → plugin bus (delivery is a plugin concern, e.g. Telegram).
  void listen("script-notify", (e) => bus.emit("notify:send", e.payload));

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
}
