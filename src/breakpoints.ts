import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { useToast } from "./toast";
import type { Flow } from "./types";

export interface Breakpoint {
  id: string;
  name: string;
  enabled: boolean;
  /** host/path glob, e.g. api.example.com/* */
  pattern: string;
  /** Method filter; null or "*" = any. */
  method: string | null;
  onRequest: boolean;
  onResponse: boolean;
  /** Owning project; null = global. */
  projectId: string | null;
}

/** A breakpoint pre-filled from a captured flow: matches its host+path (query
 *  stripped, wildcard-suffixed) and method, pausing the request phase. */
export function breakpointFromFlow(flow: Flow, projectId: string | null): Breakpoint {
  const path = flow.url.path.split("?")[0];
  return {
    id: crypto.randomUUID(),
    name: `${flow.method} ${path}`.slice(0, 40),
    enabled: true,
    pattern: `${flow.url.host}${path}*`,
    method: flow.method,
    onRequest: true,
    onResponse: false,
    projectId,
  };
}

interface BreakpointSettings {
  timeoutSecs: number;
  pauseOthers: boolean;
}

interface BreakpointsState {
  breakpoints: Breakpoint[];
  intercept: boolean;
  /** Auto-continue a paused flow after N seconds; 0 = hold forever. */
  timeoutSecs: number;
  /** Hold new requests while any flow is paused. */
  pauseOthers: boolean;
  selectedId: string | null;
  load: () => Promise<void>;
  select: (id: string | null) => void;
  upsert: (bp: Breakpoint) => Promise<void>;
  remove: (id: string) => Promise<void>;
  setIntercept: (enabled: boolean) => Promise<void>;
  saveSettings: (patch: Partial<BreakpointSettings>) => Promise<void>;
}

export const useBreakpoints = create<BreakpointsState>((set, get) => ({
  breakpoints: [],
  intercept: true,
  timeoutSecs: 0,
  pauseOthers: false,
  selectedId: null,
  load: async () => {
    const [breakpoints, intercept, settings] = await Promise.all([
      invoke<Breakpoint[]>("list_breakpoints"),
      invoke<boolean>("get_intercept"),
      invoke<BreakpointSettings>("get_breakpoint_settings"),
    ]);
    set({ breakpoints, intercept, timeoutSecs: settings.timeoutSecs, pauseOthers: settings.pauseOthers });
  },
  select: (id) => set({ selectedId: id }),
  upsert: async (bp) => {
    try {
      const breakpoints = await invoke<Breakpoint[]>("save_breakpoint", { breakpoint: bp });
      set({ breakpoints, selectedId: bp.id });
    } catch (e) {
      // Backend rejects conflicts (e.g. two enabled breakpoints on the same params).
      useToast.getState().show(String(e));
    }
  },
  remove: async (id) => {
    const breakpoints = await invoke<Breakpoint[]>("delete_breakpoint", { id });
    set({ breakpoints, selectedId: null });
  },
  setIntercept: async (enabled) => {
    await invoke("set_intercept", { enabled });
    set({ intercept: enabled });
  },
  saveSettings: async (patch) => {
    const settings = {
      timeoutSecs: patch.timeoutSecs ?? get().timeoutSecs,
      pauseOthers: patch.pauseOthers ?? get().pauseOthers,
    };
    await invoke("set_breakpoint_settings", { settings });
    set(settings);
  },
}));
