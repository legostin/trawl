import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Flow } from "./types";
import { emptyFilter, type FlowFilter } from "./filter";

export type View = "traffic" | "rules" | "breakpoints";
export type ListMode = "sequence" | "structure";

export interface EditedPayload {
  method?: string;
  /** Edited request path+query (request phase only). */
  path?: string;
  status?: number;
  headers?: [string, string][];
  body?: string;
  /** Raw body as base64 (an uploaded file); overrides `body` on the backend. */
  bodyBase64?: string;
  reason?: string;
}

interface FlowsState {
  flows: Flow[];
  selectedId: number | null;
  filter: FlowFilter;
  // UI state
  running: boolean;
  proxyAddr: string | null;
  view: View;
  listMode: ListMode;
  detailCollapsed: boolean;

  select: (id: number | null) => void;
  upsert: (flow: Flow) => void;
  setFilter: (patch: Partial<FlowFilter>) => void;
  clearFilter: () => void;
  setView: (v: View) => void;
  setListMode: (m: ListMode) => void;
  toggleDetail: () => void;
  clearFlows: () => void;

  resolveBreakpoint: (
    id: number,
    phase: "request" | "response",
    action: "execute" | "abort" | "respond",
    edited: EditedPayload,
  ) => Promise<void>;

  init: () => Promise<() => void>;
  startProxy: (port: number) => Promise<string>;
  stopProxy: () => Promise<void>;
  toggleProxy: () => Promise<void>;
  /** Start the proxy if it isn't running (used by plugin sends with viaProxy). */
  ensureProxy: () => Promise<void>;
}

export const useFlows = create<FlowsState>((set, get) => ({
  flows: [],
  selectedId: null,
  filter: emptyFilter,
  running: false,
  proxyAddr: null,
  view: "traffic",
  listMode: "sequence",
  detailCollapsed: false,

  select: (id) => set({ selectedId: id }),
  setFilter: (patch) => set((s) => ({ filter: { ...s.filter, ...patch } })),
  clearFilter: () => set({ filter: emptyFilter }),
  setView: (v) => set({ view: v }),
  setListMode: (m) => set({ listMode: m }),
  toggleDetail: () => set((s) => ({ detailCollapsed: !s.detailCollapsed })),
  clearFlows: () => set({ flows: [], selectedId: null }),

  upsert: (flow) =>
    set((s) => {
      const idx = s.flows.findIndex((f) => f.id === flow.id);
      if (idx === -1) return { flows: [...s.flows, flow] };
      const next = s.flows.slice();
      next[idx] = flow;
      return { flows: next };
    }),

  resolveBreakpoint: async (id, phase, action, edited) => {
    await invoke("resolve_breakpoint", { id, phase, action, edited });
  },

  init: async () => {
    const existing = await invoke<Flow[]>("get_flows");
    set({ flows: existing });
    const un1 = await listen<Flow>("flow-added", (e) => get().upsert(e.payload));
    const un2 = await listen<Flow>("flow-updated", (e) => get().upsert(e.payload));
    const un3 = await listen<Flow>("flow-paused", (e) => {
      get().upsert(e.payload);
      set({ selectedId: e.payload.id });
    });
    return () => {
      un1();
      un2();
      un3();
    };
  },
  startProxy: (port) => invoke<string>("start_proxy", { port }),
  stopProxy: () => invoke<void>("stop_proxy"),
  toggleProxy: async () => {
    const { running, startProxy, stopProxy } = get();
    if (running) {
      await stopProxy();
      set({ running: false, proxyAddr: null });
    } else {
      const addr = await startProxy(8729);
      set({ running: true, proxyAddr: addr });
    }
  },
  ensureProxy: async () => {
    const { running, startProxy } = get();
    if (running) return;
    const addr = await startProxy(8729);
    set({ running: true, proxyAddr: addr });
  },
}));
