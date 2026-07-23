import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { McpToolSpec } from "./api";

/** The plugin whose bundle is executing right now (set by the loader). Registering
 *  MCP tools is only allowed at this moment — that's how a tool gets attributed to a plugin. */
let loadingPluginId: string | null = null;
const handlers = new Map<string, McpToolSpec["handler"]>(); // key: `${pluginId}_${name}`

export function setLoadingPlugin(id: string | null): void {
  loadingPluginId = id;
}

export async function registerTool(spec: McpToolSpec): Promise<void> {
  if (!loadingPluginId) {
    throw new Error("mcp.registerTool must be called during plugin initialization");
  }
  const pluginId = loadingPluginId;
  handlers.set(`${pluginId}_${spec.name}`, spec.handler);
  await invoke("mcp_register_tool", {
    pluginId,
    name: spec.name,
    description: spec.description,
    inputSchema: spec.inputSchema,
    timeoutMs: spec.timeoutMs ?? null,
  });
}

export async function unregisterTool(name: string): Promise<void> {
  if (!loadingPluginId) {
    throw new Error("mcp.unregisterTool must be called during plugin initialization");
  }
  handlers.delete(`${loadingPluginId}_${name}`);
  await invoke("mcp_unregister_tool", { pluginId: loadingPluginId, name });
}

/** Remove all of a plugin's tools (before reloading its bundle and on disable). */
export async function clearPluginTools(pluginId: string): Promise<void> {
  for (const key of [...handlers.keys()]) {
    if (key.startsWith(`${pluginId}_`)) handlers.delete(key);
  }
  await invoke("mcp_clear_plugin_tools", { pluginId });
}

type ToolCallEvent = { callId: number; tool: string; args: unknown };

export async function handleToolCall(e: ToolCallEvent): Promise<void> {
  const handler = handlers.get(e.tool);
  if (!handler) {
    await invoke("mcp_tool_result", {
      callId: e.callId,
      result: null,
      error: `no handler for ${e.tool}`,
    });
    return;
  }
  try {
    const result = await handler(e.args);
    await invoke("mcp_tool_result", { callId: e.callId, result: result ?? null, error: null });
  } catch (err) {
    await invoke("mcp_tool_result", { callId: e.callId, result: null, error: String(err) });
  }
}

let listening = false;

export function initMcpBridge(): void {
  if (listening) return;
  listening = true;
  void listen<ToolCallEvent>("mcp:tool-call", (e) => void handleToolCall(e.payload));
}
