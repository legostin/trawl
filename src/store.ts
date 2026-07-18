import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Flow } from "./types";

interface FlowsState {
  flows: Flow[];
  selectedId: number | null;
  select: (id: number) => void;
  upsert: (flow: Flow) => void;
  init: () => Promise<() => void>;
  startProxy: (port: number) => Promise<string>;
  stopProxy: () => Promise<void>;
}

export const useFlows = create<FlowsState>((set, get) => ({
  flows: [],
  selectedId: null,
  select: (id) => set({ selectedId: id }),
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
}));
