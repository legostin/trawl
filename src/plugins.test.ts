import { describe, it, expect, vi, beforeEach } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

const clearPluginTools = vi.fn().mockResolvedValue(undefined);
vi.mock("./plugins/mcpBridge", () => ({ clearPluginTools: (...a: unknown[]) => clearPluginTools(...a) }));

import { cmpVersions, usePlugins } from "./plugins";

describe("cmpVersions", () => {
  it("detects a newer version", () => {
    expect(cmpVersions("0.1.1", "0.1.0")).toBeGreaterThan(0);
    expect(cmpVersions("1.0.0", "0.9.9")).toBeGreaterThan(0);
    expect(cmpVersions("0.2.0", "0.1.9")).toBeGreaterThan(0);
  });

  it("detects equal and older versions", () => {
    expect(cmpVersions("1.2.3", "1.2.3")).toBe(0);
    expect(cmpVersions("0.1.0", "0.1.1")).toBeLessThan(0);
  });

  it("handles differing segment counts", () => {
    expect(cmpVersions("1.0", "1.0.0")).toBe(0);
    expect(cmpVersions("1.0.1", "1.0")).toBeGreaterThan(0);
  });
});

describe("usePlugins.remove", () => {
  beforeEach(() => {
    invoke.mockReset();
    clearPluginTools.mockClear();
  });

  it("clears the plugin's MCP tools from the bridge on uninstall", async () => {
    invoke.mockResolvedValue([]);
    await usePlugins.getState().remove("my-plugin");
    expect(invoke).toHaveBeenCalledWith("remove_plugin", { id: "my-plugin" });
    expect(clearPluginTools).toHaveBeenCalledWith("my-plugin");
  });
});
