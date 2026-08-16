import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import {
  agentSend,
  agentInterrupt,
  agentReset,
  agentHarnesses,
  type AgentEvent,
  type HarnessAvailability,
} from "./agent";
import { applyEvent, type ChatItem } from "./chatItems";

interface AgentStore {
  items: ChatItem[];
  running: boolean;
  /** Null until the harness has told us whether it reached our MCP server. */
  trawlConnected: boolean | null;
  /** Null while unknown — the panel must not accuse the user of missing a
   *  harness before it has actually looked. */
  harnesses: HarnessAvailability[] | null;
  checkHarnesses: () => Promise<void>;
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
  harnesses: null,
  checkHarnesses: async () => {
    try {
      set({ harnesses: await agentHarnesses() });
    } catch {
      // Outside Tauri there is nothing to look at; leaving this null keeps the
      // panel quiet rather than claiming nothing is installed.
    }
  },
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
