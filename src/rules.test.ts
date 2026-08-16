import { describe, it, expect, vi } from "vitest";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
import { pickSelectedRule, type Rule } from "./rules";

const rule = (id: string, projectId: string | null): Rule => ({
  id,
  name: id,
  enabled: true,
  pattern: "*/*",
  phase: "handler",
  script: "",
  projectId,
});

const rules = [rule("a", "p1"), rule("b", "p2"), rule("global", null)];

describe("pickSelectedRule", () => {
  it("finds a rule belonging to the active project", () => {
    expect(pickSelectedRule(rules, "a", "p1")).toEqual({ rule: rules[0], outOfScope: false });
  });

  it("still finds one from another project, and says so", () => {
    // A link from a chat can point at any rule. Resolving only against the
    // visible list is what made such a link open the tab and nothing else.
    expect(pickSelectedRule(rules, "b", "p1")).toEqual({ rule: rules[1], outOfScope: true });
  });

  it("treats a global rule as in scope only when no project is active", () => {
    expect(pickSelectedRule(rules, "global", null).outOfScope).toBe(false);
    expect(pickSelectedRule(rules, "global", "p1").outOfScope).toBe(true);
  });

  it("reports nothing selected rather than inventing a rule", () => {
    expect(pickSelectedRule(rules, "missing", "p1")).toEqual({ rule: null, outOfScope: false });
    expect(pickSelectedRule(rules, null, "p1")).toEqual({ rule: null, outOfScope: false });
  });
});
