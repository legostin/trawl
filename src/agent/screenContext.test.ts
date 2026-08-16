import { describe, it, expect, vi } from "vitest";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
import { describeScreen } from "./screenContext";

describe("describeScreen", () => {
  it("names the screen even when nothing is selected", () => {
    const out = describeScreen({ mode: "setup", view: "traffic", selected: null, flowCount: 0 });
    expect(out).toContain("<screen>");
    expect(out).toContain("Setup");
    expect(out).toContain("</screen>");
  });

  it("points at the selected flow instead of pasting it", () => {
    const out = describeScreen({
      mode: "traffic",
      view: "traffic",
      selected: { id: 42, method: "GET", url: "https://api.test/pay", status: 500 },
      flowCount: 130,
    });
    expect(out).toContain("flow id 42");
    expect(out).toContain("GET https://api.test/pay");
    expect(out).toContain("500");
    // The body is deliberately absent: the agent fetches it with get_flow.
    expect(out).not.toContain("body");
  });

  it("says how much traffic there is so the agent can judge a query", () => {
    const out = describeScreen({ mode: "traffic", view: "traffic", selected: null, flowCount: 130 });
    expect(out).toContain("130");
  });

  it("distinguishes the rules screen from the traffic screen", () => {
    const out = describeScreen({ mode: "traffic", view: "rules", selected: null, flowCount: 0 });
    expect(out).toContain("Rules");
    expect(out).not.toContain("Traffic capture");
  });

  it("names the active project so the agent knows what it is scoped to", () => {
    const out = describeScreen({
      mode: "traffic",
      view: "traffic",
      selected: null,
      flowCount: 5,
      project: { id: "p1", name: "Checkout" },
      sessionStartedMs: null,
    });
    expect(out).toContain("Checkout");
  });

  it("marks where the current session begins, so recent traffic can win", () => {
    const out = describeScreen({
      mode: "traffic",
      view: "traffic",
      selected: null,
      flowCount: 5,
      project: null,
      sessionStartedMs: 1786882070269,
    });
    expect(out).toContain("1786882070269");
  });

  it("says so plainly when no project is active, instead of staying silent", () => {
    // Silence would read as "scoped to something"; the agent should know its
    // answers span every project.
    const out = describeScreen({
      mode: "traffic",
      view: "traffic",
      selected: null,
      flowCount: 5,
      project: null,
      sessionStartedMs: null,
    });
    expect(out).toContain("no project");
  });

  it("omits a status that has not arrived yet rather than inventing one", () => {
    const out = describeScreen({
      mode: "traffic",
      view: "traffic",
      selected: { id: 7, method: "POST", url: "https://api.test/slow", status: null },
      flowCount: 1,
    });
    expect(out).toContain("POST https://api.test/slow");
    expect(out).not.toContain("→");
  });
});
