import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const loaded: string[] = [];
vi.mock("./rules", () => ({ useRules: { getState: () => ({ load: async () => void loaded.push("rules") }) } }));
vi.mock("./projects", () => ({
  useProjects: { getState: () => ({ load: async () => void loaded.push("projects") }) },
}));
vi.mock("./breakpoints", () => ({
  useBreakpoints: { getState: () => ({ load: async () => void loaded.push("breakpoints") }) },
}));

const loadPlugin = vi.fn(async () => {});
const loadEnabledPlugins = vi.fn(async () => {});
const forgetPlugin = vi.fn(async () => {});
let installed: { id: string; enabled: boolean }[] = [];
vi.mock("./plugins", () => ({
  usePlugins: { getState: () => ({ load: async () => void loaded.push("plugins"), installed }) },
  forgetPlugin,
}));
vi.mock("./plugins/loader", () => ({ loadPlugin, loadEnabledPlugins }));

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

/** applyChange fires the reload without awaiting it. */
const settle = () => new Promise((r) => setTimeout(r, 0));

describe("a plugin changed underneath the window", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    loaded.length = 0;
    installed = [];
  });

  it("reloads only the plugin that changed", async () => {
    installed = [
      { id: "a", enabled: true },
      { id: "b", enabled: true },
    ];
    applyChange("plugins", "b");
    await settle();
    expect(loadPlugin).toHaveBeenCalledWith("b");
    expect(loadEnabledPlugins).not.toHaveBeenCalled();
  });

  it("tears down a plugin that is gone from the registry", async () => {
    // Absent means deleted — inference beats a second payload field both
    // sides would have to agree on.
    applyChange("plugins", "gone");
    await settle();
    expect(forgetPlugin).toHaveBeenCalledWith("gone");
    expect(loadPlugin).not.toHaveBeenCalled();
  });

  it("leaves a disabled plugin alone", async () => {
    installed = [{ id: "a", enabled: false }];
    applyChange("plugins", "a");
    await settle();
    expect(loadPlugin).not.toHaveBeenCalled();
    expect(forgetPlugin).not.toHaveBeenCalled();
  });

  it("falls back to loading everything when no id is named", async () => {
    applyChange("plugins");
    await settle();
    expect(loadEnabledPlugins).toHaveBeenCalled();
  });
});
