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
} from "@/db";
import { useFlows } from "@/store";
import { useProjects } from "@/projects";
import { usePlugins } from "@/plugins";
import { bus } from "./bus";
import type { RegisteredMode, TrawlHost } from "./api";

const HOST_VERSION = "1.0.0";

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
      query: (f, limit, offset) => queryFlows(f, limit, offset),
      count: (f) => flowCount(f),
      aggregate: (f, g, bucket, limit) => aggregateFlows(f, g, bucket, limit),
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
    registerMode: (mode: RegisteredMode) => usePlugins.getState().registerMode(mode),
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
