import { describe, it, expect, vi } from "vitest";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
import { applyEvent, type ChatItem } from "./chatItems";

const empty: ChatItem[] = [];

describe("applyEvent", () => {
  it("opens an assistant item on the first text", () => {
    const items = applyEvent(empty, { kind: "assistantText", text: "Looking" });
    expect(items).toEqual([{ type: "assistant", text: "Looking" }]);
  });

  it("appends later text to the same item rather than stacking bubbles", () => {
    let items = applyEvent(empty, { kind: "assistantText", text: "Looking" });
    items = applyEvent(items, { kind: "assistantText", text: " at the logs" });
    expect(items).toEqual([{ type: "assistant", text: "Looking at the logs" }]);
  });

  it("shows a tool call as its own item so the user sees the work", () => {
    const items = applyEvent(empty, {
      kind: "toolCall",
      id: "t1",
      name: "mcp__trawl__query_flows",
      input: { status: 500 },
    });
    expect(items).toEqual([
      { type: "tool", id: "t1", name: "query_flows", input: { status: 500 } },
    ]);
  });

  it("closes the assistant item at the end of a turn, so the next reply is separate", () => {
    let items = applyEvent(empty, { kind: "assistantText", text: "done" });
    items = applyEvent(items, {
      kind: "turnDone",
      text: "done",
      usage: { inputTokens: 1, outputTokens: 2 },
    });
    items = applyEvent(items, { kind: "assistantText", text: "second reply" });
    expect(items).toHaveLength(2);
    expect(items[1]).toEqual({ type: "assistant", text: "second reply" });
  });

  it("keeps an error visible instead of replacing the conversation", () => {
    let items = applyEvent(empty, { kind: "assistantText", text: "partial" });
    items = applyEvent(items, { kind: "error", message: "rate limited" });
    expect(items).toEqual([
      { type: "assistant", text: "partial" },
      { type: "error", message: "rate limited" },
    ]);
  });

  it("ignores reasoning: it is not the answer and clutters the column", () => {
    expect(applyEvent(empty, { kind: "reasoning", text: "hmm" })).toEqual(empty);
  });

  it("does not merge text across a tool call, which happened in between", () => {
    let items = applyEvent(empty, { kind: "assistantText", text: "checking" });
    items = applyEvent(items, {
      kind: "toolCall",
      id: "t1",
      name: "mcp__trawl__get_flow",
      input: {},
    });
    items = applyEvent(items, { kind: "assistantText", text: "found it" });
    expect(items.map((i) => i.type)).toEqual(["assistant", "tool", "assistant"]);
  });
});
