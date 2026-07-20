import * as React from "react";
import * as JsxRuntime from "react/jsx-runtime";
import { listen } from "@tauri-apps/api/event";
import {
  aggregateFlows,
  deleteReport,
  flowCount,
  listReports,
  queryFlows,
  saveReport,
  type FlowQuery,
} from "@/db";
import { useFlows } from "@/store";
import { useProjects } from "@/projects";
import { useLayout } from "@/layout";
import { usePlugins } from "@/plugins";
import { sendRequest } from "@/http";
import { bodyToText } from "@/lib/body";
import { buildCurl } from "@/lib/curl";
import { BodyViewer } from "@/components/BodyViewer";
import { HeadersTable } from "@/components/HeadersTable";
import { MethodBadge, StatusBadge } from "@/components/badges";
import { bus } from "./bus";
import type { FlowAction, RegisteredMode, TrawlHost } from "./api";

const HOST_VERSION = "1.1.0";

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
    ui: { BodyViewer, HeadersTable, MethodBadge, StatusBadge },
    util: {
      bodyText: (msg) => bodyToText(msg),
      buildCurl: (flow) => buildCurl(flow),
    },
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
