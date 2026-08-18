import { describe, it, expect } from "vitest";
import { flowTabs, resolveTab } from "./flowTabs";

describe("flow detail tabs", () => {
  it("keeps the core tabs in order when no plugin contributes", () => {
    expect(flowTabs([]).map((t) => t.value)).toEqual(["overview", "request", "response", "timing"]);
  });

  it("appends plugin tabs after the core ones, namespaced", () => {
    // Namespacing keeps a plugin from ever colliding with a core tab.
    const tabs = flowTabs([{ id: "openapi", label: "OpenAPI" }]);
    expect(tabs[tabs.length - 1]).toEqual({ value: "plugin:openapi", label: "OpenAPI" });
  });

  it("falls back to overview when the active tab disappears", () => {
    // A plugin disabled while its tab was open must not blank the card.
    const tabs = flowTabs([]);
    expect(resolveTab("plugin:openapi", tabs)).toBe("overview");
  });

  it("keeps the active tab when it still exists", () => {
    const tabs = flowTabs([{ id: "openapi", label: "OpenAPI" }]);
    expect(resolveTab("plugin:openapi", tabs)).toBe("plugin:openapi");
    expect(resolveTab("request", tabs)).toBe("request");
  });
});
