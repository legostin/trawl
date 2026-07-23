import { describe, it, expect, vi, beforeEach } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

import { overriddenKeys, useProjects } from "./projects";

describe("overriddenKeys", () => {
  it("returns project keys that shadow a global key", () => {
    const g = [
      { key: "HOST", value: "g" },
      { key: "TOKEN", value: "g" },
    ];
    const p = [
      { key: "TOKEN", value: "p" },
      { key: "LOCAL", value: "p" },
      { key: "", value: "" },
    ];
    expect(overriddenKeys(g, p)).toEqual(new Set(["TOKEN"]));
  });
});

describe("projects store — global env", () => {
  beforeEach(() => {
    invoke.mockReset();
    useProjects.setState({ projects: [], activeId: null, globalEnv: [], variablesOpen: false });
  });

  it("load() picks up globalEnv (missing field → [])", async () => {
    invoke.mockResolvedValue({ projects: [], activeId: null, globalEnv: [{ key: "G", value: "1" }] });
    await useProjects.getState().load();
    expect(useProjects.getState().globalEnv).toEqual([{ key: "G", value: "1" }]);

    invoke.mockResolvedValue({ projects: [], activeId: null });
    await useProjects.getState().load();
    expect(useProjects.getState().globalEnv).toEqual([]);
  });

  it("saveGlobalEnv invokes the command and stores the result", async () => {
    invoke.mockResolvedValue({ projects: [], activeId: null, globalEnv: [{ key: "A", value: "2" }] });
    await useProjects.getState().saveGlobalEnv([{ key: "A", value: "2" }]);
    expect(invoke).toHaveBeenCalledWith("save_global_env", { env: [{ key: "A", value: "2" }] });
    expect(useProjects.getState().globalEnv).toEqual([{ key: "A", value: "2" }]);
  });

  it("openVariables/closeVariables toggle the flag", () => {
    useProjects.getState().openVariables();
    expect(useProjects.getState().variablesOpen).toBe(true);
    useProjects.getState().closeVariables();
    expect(useProjects.getState().variablesOpen).toBe(false);
  });
});
