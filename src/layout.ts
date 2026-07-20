import { create } from "zustand";

/** Top-level app mode selected in the sidebar. Extend as new modes are added. */
export type Mode = "traffic";

const COLLAPSE_KEY = "trawl-sidebar-collapsed";

function initialCollapsed(): boolean {
  try {
    return localStorage.getItem(COLLAPSE_KEY) === "1";
  } catch {
    return false;
  }
}

interface LayoutState {
  mode: Mode;
  setMode: (m: Mode) => void;
  sidebarCollapsed: boolean;
  toggleSidebar: () => void;
}

export const useLayout = create<LayoutState>((set) => ({
  mode: "traffic",
  setMode: (mode) => set({ mode }),
  sidebarCollapsed: initialCollapsed(),
  toggleSidebar: () =>
    set((s) => {
      const sidebarCollapsed = !s.sidebarCollapsed;
      try {
        localStorage.setItem(COLLAPSE_KEY, sidebarCollapsed ? "1" : "0");
      } catch {
        /* ignore */
      }
      return { sidebarCollapsed };
    }),
}));
