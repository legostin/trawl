import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";

/** Minimal in-memory Storage stub (vitest runs in the `node` environment). */
function memStorage(): Storage {
  const m = new Map<string, string>();
  return {
    getItem: (k) => (m.has(k) ? m.get(k)! : null),
    setItem: (k, v) => void m.set(k, v),
    removeItem: (k) => void m.delete(k),
    clear: () => m.clear(),
    key: () => null,
    get length() {
      return m.size;
    },
  } as Storage;
}

beforeEach(() => vi.resetModules());
afterEach(() => vi.unstubAllGlobals());

describe("useLayout", () => {
  it("defaults to expanded when nothing is stored", async () => {
    vi.stubGlobal("localStorage", memStorage());
    const { useLayout } = await import("./layout");
    expect(useLayout.getState().sidebarCollapsed).toBe(false);
    expect(useLayout.getState().mode).toBe("traffic");
  });

  it("reads the collapsed flag from localStorage on init", async () => {
    const s = memStorage();
    s.setItem("trawl-sidebar-collapsed", "1");
    vi.stubGlobal("localStorage", s);
    const { useLayout } = await import("./layout");
    expect(useLayout.getState().sidebarCollapsed).toBe(true);
  });

  it("toggleSidebar flips the flag and persists it", async () => {
    const s = memStorage();
    vi.stubGlobal("localStorage", s);
    const { useLayout } = await import("./layout");

    useLayout.getState().toggleSidebar();
    expect(useLayout.getState().sidebarCollapsed).toBe(true);
    expect(s.getItem("trawl-sidebar-collapsed")).toBe("1");

    useLayout.getState().toggleSidebar();
    expect(useLayout.getState().sidebarCollapsed).toBe(false);
    expect(s.getItem("trawl-sidebar-collapsed")).toBe("0");
  });

  it("setMode updates the active mode", async () => {
    vi.stubGlobal("localStorage", memStorage());
    const { useLayout } = await import("./layout");
    useLayout.getState().setMode("traffic");
    expect(useLayout.getState().mode).toBe("traffic");
  });
});
