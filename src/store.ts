import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Flow } from "./types";
import { emptyFilter, type FlowFilter } from "./filter";

export type View = "traffic" | "setup";
export type ListMode = "sequence" | "structure";

interface FlowsState {
  flows: Flow[];
  selectedId: number | null;
  filter: FlowFilter;
  // UI-состояние
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

  init: () => Promise<() => void>;
  startProxy: (port: number) => Promise<string>;
  stopProxy: () => Promise<void>;
  toggleProxy: () => Promise<void>;
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

  init: async () => {
    const existing = await invoke<Flow[]>("get_flows");
    set({ flows: existing });
    const un1 = await listen<Flow>("flow-added", (e) => get().upsert(e.payload));
    const un2 = await listen<Flow>("flow-updated", (e) => get().upsert(e.payload));
    return () => {
      un1();
      un2();
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
      const addr = await startProxy(8888);
      set({ running: true, proxyAddr: addr });
    }
  },
}));
