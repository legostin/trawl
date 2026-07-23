import { describe, it, expect, vi, beforeEach } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

import { useBreakpoints, breakpointFromFlow } from "./breakpoints";
import type { Flow } from "./types";

const bp = (id: string) => ({
  id, name: "n", enabled: true, pattern: "*/*",
  method: null, onRequest: true, onResponse: false, projectId: null,
});

describe("breakpoints store", () => {
  beforeEach(() => {
    invoke.mockReset();
    useBreakpoints.setState({ breakpoints: [], intercept: true });
  });

  it("load pulls definitions and intercept flag", async () => {
    invoke.mockImplementation((cmd: string) =>
      cmd === "list_breakpoints" ? Promise.resolve([bp("a")]) : Promise.resolve(true));
    await useBreakpoints.getState().load();
    expect(useBreakpoints.getState().breakpoints).toHaveLength(1);
    expect(useBreakpoints.getState().intercept).toBe(true);
  });

  it("upsert saves and stores the returned list", async () => {
    invoke.mockResolvedValue([bp("a"), bp("b")]);
    await useBreakpoints.getState().upsert(bp("b"));
    expect(invoke).toHaveBeenCalledWith("save_breakpoint", { breakpoint: bp("b") });
    expect(useBreakpoints.getState().breakpoints).toHaveLength(2);
  });

  it("setIntercept invokes and updates state", async () => {
    invoke.mockResolvedValue(undefined);
    await useBreakpoints.getState().setIntercept(false);
    expect(invoke).toHaveBeenCalledWith("set_intercept", { enabled: false });
    expect(useBreakpoints.getState().intercept).toBe(false);
  });
});

describe("breakpointFromFlow", () => {
  const flow = (patch: Partial<Flow> = {}): Flow => ({
    id: 1, timestamp: 0, method: "POST",
    url: { scheme: "https", host: "api.example.com", port: 443, path: "/v1/users?page=2" },
    request: { headers: [], body: "", bodyIsText: true },
    response: null,
    timings: { sent: null, ttfb: null, done: null },
    state: "completed", error: null, appliedRules: [], ruleTrace: [], ...patch,
  });

  it("derives pattern from host+path (query stripped) and method from the flow", () => {
    const b = breakpointFromFlow(flow(), "proj-1");
    expect(b.pattern).toBe("api.example.com/v1/users*");
    expect(b.method).toBe("POST");
    expect(b.onRequest).toBe(true);
    expect(b.onResponse).toBe(false);
    expect(b.enabled).toBe(true);
    expect(b.projectId).toBe("proj-1");
    expect(b.id).toBeTruthy();
  });

  it("defaults projectId to null when no active project", () => {
    const b = breakpointFromFlow(flow(), null);
    expect(b.projectId).toBeNull();
  });
});
