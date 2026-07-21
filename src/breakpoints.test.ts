import { describe, it, expect, vi, beforeEach } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

import { useBreakpoints } from "./breakpoints";

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
