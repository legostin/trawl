import { describe, it, expect, vi } from "vitest";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
import { mcpAddCommand } from "./mcp";

describe("mcpAddCommand", () => {
  it("builds the claude mcp add command", () => {
    expect(mcpAddCommand(9910, "abc")).toBe(
      'claude mcp add --transport http trawl http://127.0.0.1:9910/mcp --header "Authorization: Bearer abc"',
    );
  });
});
