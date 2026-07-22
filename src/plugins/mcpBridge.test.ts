import { describe, it, expect, vi, beforeEach } from "vitest";

const invoke = vi.fn().mockResolvedValue(undefined);
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

import {
  registerTool,
  setLoadingPlugin,
  clearPluginTools,
  handleToolCall,
} from "./mcpBridge";

describe("mcpBridge", () => {
  beforeEach(() => {
    invoke.mockClear();
    setLoadingPlugin(null);
  });

  it("rejects registerTool outside plugin initialization", async () => {
    await expect(
      registerTool({ name: "t", description: "", inputSchema: {}, handler: () => null }),
    ).rejects.toThrow(/initialization/);
  });

  it("registers with the loading plugin id and dispatches calls", async () => {
    setLoadingPlugin("my-plugin");
    await registerTool({
      name: "echo",
      description: "d",
      inputSchema: { type: "object" },
      handler: (args) => ({ got: args }),
    });
    setLoadingPlugin(null);
    expect(invoke).toHaveBeenCalledWith(
      "mcp_register_tool",
      expect.objectContaining({ pluginId: "my-plugin", name: "echo" }),
    );

    await handleToolCall({ callId: 5, tool: "my-plugin_echo", args: { x: 1 } });
    expect(invoke).toHaveBeenCalledWith("mcp_tool_result", {
      callId: 5,
      result: { got: { x: 1 } },
      error: null,
    });
  });

  it("reports handler errors", async () => {
    setLoadingPlugin("p");
    await registerTool({
      name: "boom",
      description: "",
      inputSchema: {},
      handler: () => {
        throw new Error("nope");
      },
    });
    setLoadingPlugin(null);
    await handleToolCall({ callId: 1, tool: "p_boom", args: {} });
    expect(invoke).toHaveBeenCalledWith("mcp_tool_result", {
      callId: 1,
      result: null,
      error: expect.stringContaining("nope"),
    });
  });

  it("clearPluginTools drops handlers and reports missing handler", async () => {
    setLoadingPlugin("p2");
    await registerTool({ name: "t", description: "", inputSchema: {}, handler: () => 1 });
    setLoadingPlugin(null);
    await clearPluginTools("p2");
    expect(invoke).toHaveBeenCalledWith("mcp_clear_plugin_tools", { pluginId: "p2" });
    await handleToolCall({ callId: 2, tool: "p2_t", args: {} });
    expect(invoke).toHaveBeenCalledWith("mcp_tool_result", {
      callId: 2,
      result: null,
      error: expect.stringContaining("no handler"),
    });
  });
});
