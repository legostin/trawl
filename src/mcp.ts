import { invoke } from "@tauri-apps/api/core";

export interface McpConfig {
  enabled: boolean;
  port: number;
  token: string;
}

export interface McpServerStatus {
  running: boolean;
  error: string | null;
}

export const getMcpConfig = () => invoke<McpConfig>("mcp_get_config");
export const setMcpConfig = (enabled: boolean, port: number) =>
  invoke<McpConfig>("mcp_set_config", { enabled, port });
export const regenMcpToken = () => invoke<McpConfig>("mcp_regen_token");
export const mcpServerStatus = () => invoke<McpServerStatus>("mcp_server_status");

export function mcpAddCommand(port: number, token: string): string {
  return `claude mcp add --transport http trawl http://127.0.0.1:${port}/mcp --header "Authorization: Bearer ${token}"`;
}
