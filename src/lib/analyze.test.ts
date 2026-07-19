import { describe, it, expect } from "vitest";
import { analyzeJson, accessor, matchGlob } from "./analyze";

describe("analyzeJson", () => {
  it("collects nested object field paths, types and examples", () => {
    const fields = analyzeJson([{ user: { name: "a", age: 3 } }]);
    expect(fields).toEqual([
      { path: "user.age", type: "number", example: "3", varying: false },
      { path: "user.name", type: "string", example: "a", varying: false },
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

  it("marks a field as varying when its value differs across responses", () => {
    const stable = analyzeJson([{ token: "x" }, { token: "x" }]).find((f) => f.path === "token");
    expect(stable?.varying).toBe(false);
    const dynamic = analyzeJson([{ token: "a" }, { token: "b" }]).find((f) => f.path === "token");
    expect(dynamic?.varying).toBe(true);
    expect(dynamic?.example).toBe("b"); // последнее значение
  });

  it("truncates long example values", () => {
    const long = "x".repeat(80);
    const f = analyzeJson([{ blob: long }])[0];
    expect(f.example!.length).toBeLessThan(50);
    expect(f.example!.endsWith("…")).toBe(true);
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
