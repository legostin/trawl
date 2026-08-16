import { invoke } from "@tauri-apps/api/core";

export interface Usage {
  inputTokens: number;
  outputTokens: number;
}

export type AgentEvent =
  | { kind: "sessionStarted"; sessionId: string; model: string; trawlConnected: boolean }
  | { kind: "assistantText"; text: string }
  | { kind: "reasoning"; text: string }
  | { kind: "toolCall"; id: string; name: string; input: unknown }
  | { kind: "turnDone"; text: string; usage: Usage }
  | { kind: "error"; message: string };

export const agentSend = (text: string, screenContext: string) =>
  invoke<void>("agent_send", { text, screenContext });
export const agentInterrupt = () => invoke<void>("agent_interrupt");
export const agentReset = () => invoke<void>("agent_reset");
