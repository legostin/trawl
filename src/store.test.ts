import { describe, it, expect, vi, beforeEach } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

import { useFlows } from "./store";
import type { Flow } from "./types";

const flow = (id: number, patch: Partial<Flow> = {}): Flow => ({
  id, timestamp: 0, method: "GET",
  url: { scheme: "http", host: "h", port: 80, path: "/" },
  request: { headers: [], body: "", bodyIsText: true },
  response: null,
  timings: { sent: null, ttfb: null, done: null },
  state: "pending", error: null, appliedRules: [], ...patch,
});

describe("flows store — breakpoints", () => {
  beforeEach(() => {
    invoke.mockReset();
    useFlows.setState({ flows: [], selectedId: null });
  });

  it("upsert reflects a paused flow", () => {
    useFlows.getState().upsert(flow(1, { state: "paused", pausedPhase: "request" }));
    const f = useFlows.getState().flows[0];
    expect(f.state).toBe("paused");
    expect(f.pausedPhase).toBe("request");
  });

  it("resolveBreakpoint invokes resolve_breakpoint with the payload", async () => {
    invoke.mockResolvedValue(undefined);
    await useFlows.getState().resolveBreakpoint(1, "request", "abort", { reason: "no" });
    expect(invoke).toHaveBeenCalledWith("resolve_breakpoint", {
      id: 1, phase: "request", action: "abort", edited: { reason: "no" },
    });
  });
});
