import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

const loaded: string[] = [];
vi.mock("./rules", () => ({ useRules: { getState: () => ({ load: async () => void loaded.push("rules") }) } }));
vi.mock("./projects", () => ({
  useProjects: { getState: () => ({ load: async () => void loaded.push("projects") }) },
}));
vi.mock("./breakpoints", () => ({
  useBreakpoints: { getState: () => ({ load: async () => void loaded.push("breakpoints") }) },
}));

const { applyChange } = await import("./liveState");

describe("changes made outside the window", () => {
  beforeEach(() => void (loaded.length = 0));

  it("reloads the part that changed, and only that part", () => {
    applyChange("rules");
    expect(loaded).toEqual(["rules"]);
  });

  it("knows every domain the backend can report", () => {
    applyChange("projects");
    applyChange("breakpoints");
    expect(loaded).toEqual(["projects", "breakpoints"]);
  });

  it("ignores something it does not know rather than throwing", () => {
    // A newer backend may report a domain this window has no store for.
    expect(() => applyChange("weather")).not.toThrow();
    expect(loaded).toEqual([]);
  });
});
