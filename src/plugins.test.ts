import { describe, it, expect, vi, beforeEach } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

const clearPluginTools = vi.fn().mockResolvedValue(undefined);
vi.mock("./plugins/mcpBridge", () => ({ clearPluginTools: (...a: unknown[]) => clearPluginTools(...a) }));

import { apiCompatible, cmpVersions, HOST_API_VERSION, usePlugins, type Plugin } from "./plugins";

function plug(id: string, repo: string, version: string): Plugin {
  return {
    id,
    name: id,
    version,
    description: "",
    author: "",
    repo,
    host: "github.com",
    ref: "main",
    enabled: true,
  };
}

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

describe("apiCompatible", () => {
  it("accepts a missing, empty, equal or older apiVersion", () => {
    expect(apiCompatible(undefined)).toBe(true);
    expect(apiCompatible("")).toBe(true);
    expect(apiCompatible(HOST_API_VERSION)).toBe(true);
    expect(apiCompatible("1.0.0")).toBe(true);
  });

  it("rejects an apiVersion newer than the host's", () => {
    expect(apiCompatible("999.0.0")).toBe(false);
  });
});

describe("usePlugins.checkUpdates", () => {
  beforeEach(() => {
    invoke.mockReset();
    usePlugins.setState({ installed: [], updates: {}, blockedUpdates: {} });
  });

  it("offers compatible updates and blocks ones that need a newer app", async () => {
    usePlugins.setState({
      installed: [plug("ok", "o/ok", "0.1.0"), plug("needs-app", "o/na", "0.1.0")],
    });
    invoke.mockImplementation(async (_cmd: unknown, args: unknown) => {
      const { repo } = args as { repo: string };
      return repo === "o/ok"
        ? { id: "ok", version: "0.2.0", apiVersion: HOST_API_VERSION }
        : { id: "needs-app", version: "0.9.0", apiVersion: "999.0.0" };
    });
    await usePlugins.getState().checkUpdates();
    expect(usePlugins.getState().updates).toEqual({ ok: "0.2.0" });
    expect(usePlugins.getState().blockedUpdates).toEqual({
      "needs-app": { version: "0.9.0", apiVersion: "999.0.0" },
    });
  });

  it("does not offer an update at all when versions are equal", async () => {
    usePlugins.setState({ installed: [plug("same", "o/same", "0.2.0")] });
    invoke.mockResolvedValue({ id: "same", version: "0.2.0", apiVersion: "999.0.0" });
    await usePlugins.getState().checkUpdates();
    expect(usePlugins.getState().updates).toEqual({});
    expect(usePlugins.getState().blockedUpdates).toEqual({});
  });
});

describe("host API version forwarding", () => {
  beforeEach(() => {
    invoke.mockReset();
    usePlugins.setState({ installed: [], updates: {}, blockedUpdates: {} });
  });

  it("install passes hostApiVersion to the backend", async () => {
    invoke.mockResolvedValue([]);
    await usePlugins.getState().install("o/r");
    expect(invoke).toHaveBeenCalledWith("install_plugin", {
      repo: "o/r",
      reference: undefined,
      hostApiVersion: HOST_API_VERSION,
    });
  });

  it("update passes hostApiVersion to the backend", async () => {
    usePlugins.setState({ installed: [plug("a", "o/a", "0.1.0")] });
    invoke.mockResolvedValue([]);
    await usePlugins.getState().update("a");
    expect(invoke).toHaveBeenCalledWith("install_plugin", {
      repo: "o/a",
      reference: "main",
      host: "github.com",
      hostApiVersion: HOST_API_VERSION,
    });
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
