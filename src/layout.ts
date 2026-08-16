import { create } from "zustand";

/** Top-level app mode selected in the sidebar: built-ins ("traffic", "plugins")
 *  or a plugin-registered mode id. */
export type Mode = string;

const COLLAPSE_KEY = "trawl-sidebar-collapsed";
const AGENT_KEY = "trawl-agent-open";

function initialCollapsed(): boolean {
  try {
    return localStorage.getItem(COLLAPSE_KEY) === "1";
  } catch {
    return false;
  }
}

function initialAgentOpen(): boolean {
  try {
    return localStorage.getItem(AGENT_KEY) === "1";
  } catch {
    return false;
  }
}

interface LayoutState {
  mode: Mode;
  setMode: (m: Mode) => void;
  sidebarCollapsed: boolean;
  toggleSidebar: () => void;
  agentOpen: boolean;
  toggleAgent: () => void;
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
  agentOpen: initialAgentOpen(),
  toggleAgent: () =>
    set((s) => {
      const agentOpen = !s.agentOpen;
      try {
        localStorage.setItem(AGENT_KEY, agentOpen ? "1" : "0");
      } catch {
        /* ignore */
      }
      return { agentOpen };
    }),
}));
