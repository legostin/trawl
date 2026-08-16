import type { AgentEvent } from "./agent";

export type ChatItem =
  | { type: "user"; text: string }
  /** `closed` marks a finished turn, so the next reply starts a new bubble. */
  | { type: "assistant"; text: string; closed?: boolean }
  | { type: "tool"; id: string; name: string; input: unknown }
  | { type: "error"; message: string };

/**
 * Events fold into the transcript. Assistant text accumulates into the item
 * that is still open, so a streamed answer reads as one message; the end of a
 * turn closes it, so the next reply is visibly separate.
 */
export function applyEvent(items: ChatItem[], event: AgentEvent): ChatItem[] {
  switch (event.kind) {
    case "assistantText": {
      const last = items[items.length - 1];
      if (last?.type === "assistant" && !last.closed) {
        return [...items.slice(0, -1), { ...last, text: last.text + event.text }];
      }
      return [...items, { type: "assistant", text: event.text }];
    }
    case "toolCall":
      return [
        ...items,
        { type: "tool", id: event.id, name: shortToolName(event.name), input: event.input },
      ];
    case "turnDone": {
      // The text already arrived as assistantText; what a result line adds is
      // the fact that the turn is over.
      const last = items[items.length - 1];
      if (last?.type === "assistant") {
        return [...items.slice(0, -1), { ...last, closed: true }];
      }
      return items;
    }
    case "error":
      return [...items, { type: "error", message: event.message }];
    default:
      return items;
  }
}

/** `mcp__trawl__query_flows` reads as noise in the UI; `query_flows` does not. */
export function shortToolName(name: string): string {
  return name.replace(/^mcp__trawl__/, "");
}
