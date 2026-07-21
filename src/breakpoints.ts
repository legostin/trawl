import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";

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

interface BreakpointsState {
  breakpoints: Breakpoint[];
  intercept: boolean;
  selectedId: string | null;
  load: () => Promise<void>;
  select: (id: string | null) => void;
  upsert: (bp: Breakpoint) => Promise<void>;
  remove: (id: string) => Promise<void>;
  setIntercept: (enabled: boolean) => Promise<void>;
}

export const useBreakpoints = create<BreakpointsState>((set) => ({
  breakpoints: [],
  intercept: true,
  selectedId: null,
  load: async () => {
    const [breakpoints, intercept] = await Promise.all([
      invoke<Breakpoint[]>("list_breakpoints"),
      invoke<boolean>("get_intercept"),
    ]);
    set({ breakpoints, intercept });
  },
  select: (id) => set({ selectedId: id }),
  upsert: async (bp) => {
    const breakpoints = await invoke<Breakpoint[]>("save_breakpoint", { breakpoint: bp });
    set({ breakpoints, selectedId: bp.id });
  },
  remove: async (id) => {
    const breakpoints = await invoke<Breakpoint[]>("delete_breakpoint", { id });
    set({ breakpoints, selectedId: null });
  },
  setIntercept: async (enabled) => {
    await invoke("set_intercept", { enabled });
    set({ intercept: enabled });
  },
}));
