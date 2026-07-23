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
  state: "pending", error: null, appliedRules: [], ruleTrace: [], ...patch,
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

describe("flows store — ensureProxy", () => {
  beforeEach(() => {
    invoke.mockReset();
    useFlows.setState({ running: false, proxyAddr: null });
  });

  it("starts the proxy and marks it running when stopped", async () => {
    invoke.mockResolvedValue("0.0.0.0:8729");
    await useFlows.getState().ensureProxy();
    expect(invoke).toHaveBeenCalledWith("start_proxy", { port: 8729 });
    expect(useFlows.getState().running).toBe(true);
    expect(useFlows.getState().proxyAddr).toBe("0.0.0.0:8729");
  });

  it("does nothing when the proxy is already running", async () => {
    useFlows.setState({ running: true, proxyAddr: "0.0.0.0:8729" });
    await useFlows.getState().ensureProxy();
    expect(invoke).not.toHaveBeenCalled();
  });
});
