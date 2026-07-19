import { describe, it, expect } from "vitest";
import { analyzeJson, accessor, matchGlob } from "./analyze";

describe("analyzeJson", () => {
  it("collects nested object field paths and types", () => {
    const fields = analyzeJson([{ user: { name: "a", age: 3 } }]);
    expect(fields).toEqual([
      { path: "user.age", type: "number" },
      { path: "user.name", type: "string" },
    ]);
  });

  it("records array path and element fields", () => {
    const fields = analyzeJson([{ users: [{ id: 1, active: true }] }]);
    const paths = fields.map((f) => f.path);
    expect(paths).toContain("users");
    expect(paths).toContain("users[].id");
    expect(paths).toContain("users[].active");
    expect(fields.find((f) => f.path === "users")?.type).toBe("array");
  });

  it("merges fields across multiple responses", () => {
    const fields = analyzeJson([{ a: 1 }, { b: "x" }]);
    expect(fields.map((f) => f.path)).toEqual(["a", "b"]);
  });

  it("handles null and booleans", () => {
    const fields = analyzeJson([{ x: null, y: false }]);
    expect(fields).toEqual([
      { path: "x", type: "null" },
      { path: "y", type: "boolean" },
    ]);
  });
});

describe("accessor", () => {
  it("builds bracket-safe accessors", () => {
    expect(accessor("user.name")).toBe("data['user']['name']");
    expect(accessor("users[].id")).toBe("data['users'][0]['id']");
  });
});

describe("matchGlob", () => {
  it("matches wildcards", () => {
    expect(matchGlob("api.example.com/*", "api.example.com/v1")).toBe(true);
    expect(matchGlob("*/v1/*", "api.example.com/v1/users")).toBe(true);
    expect(matchGlob("api.example.com/*", "cdn.example.com/x")).toBe(false);
  });
});
