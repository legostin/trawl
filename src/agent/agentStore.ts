import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { agentSend, agentInterrupt, agentReset, type AgentEvent } from "./agent";
import { applyEvent, type ChatItem } from "./chatItems";

interface AgentStore {
  items: ChatItem[];
  running: boolean;
  /** Null until the harness has told us whether it reached our MCP server. */
  trawlConnected: boolean | null;
  init: () => Promise<void>;
  send: (text: string, screenContext: string) => Promise<void>;
  interrupt: () => Promise<void>;
  reset: () => Promise<void>;
}

let listening = false;

export const useAgent = create<AgentStore>((set) => ({
  items: [],
  running: false,
  trawlConnected: null,
  init: async () => {
    // The panel mounts once and lives for the whole app, but guard anyway:
    // a second listener would double every message in the transcript.
    if (listening) return;
    listening = true;
    await listen<AgentEvent>("agent-event", (e) => {
      const event = e.payload;
      set((s) => ({
        items: applyEvent(s.items, event),
        running: event.kind === "turnDone" || event.kind === "error" ? false : s.running,
        trawlConnected:
          event.kind === "sessionStarted" ? event.trawlConnected : s.trawlConnected,
      }));
    });
  },
  send: async (text, screenContext) => {
    set((s) => ({ items: [...s.items, { type: "user", text }], running: true }));
    try {
      await agentSend(text, screenContext);
    } catch (err) {
      set((s) => ({
        items: [...s.items, { type: "error", message: String(err) }],
        running: false,
      }));
    }
  },
  interrupt: async () => {
    await agentInterrupt();
    set({ running: false });
  },
  reset: async () => {
    await agentReset();
    set({ items: [], running: false, trawlConnected: null });
  },
}));
