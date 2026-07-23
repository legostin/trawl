import { describe, expect, it } from "vitest";
import { extractPathLiterals, pathArgContext } from "./pathContext";

describe("pathArgContext", () => {
  it("cursor inside a path string → fn and prefix", () => {
    const line = "patch(res, 'items[*].adv";
    expect(pathArgContext(line, line.length + 1)).toEqual({ fn: "patch", prefix: "items[*].adv" });
  });
  it("double quotes and an empty prefix", () => {
    const line = 'pick(res, "';
    expect(pathArgContext(line, line.length + 1)).toEqual({ fn: "pick", prefix: "" });
  });
  it("outside a path string → null", () => {
    expect(pathArgContext("patch(res, ", 12)).toBeNull();
    expect(pathArgContext("someFn(res, 'a.b", 17)).toBeNull();
    expect(pathArgContext("patch(res, 'a.b', ", 19)).toBeNull(); // string closed
  });
});

describe("extractPathLiterals", () => {
  it("finds all literals with coordinates", () => {
    const script = "const r = send(request);\npatch(r, 'items[*].x', 1);\npick(r, \"a\");";
    const lits = extractPathLiterals(script);
    expect(lits).toHaveLength(2);
    expect(lits[0]).toMatchObject({ path: "items[*].x", line: 2 });
    // columns: the path starts right after the quote
    expect(script.split("\n")[1].slice(lits[0].startColumn - 1, lits[0].endColumn - 1)).toBe("items[*].x");
    expect(lits[1]).toMatchObject({ path: "a", line: 3 });
  });
  it("dynamic path (variable) is skipped", () => {
    expect(extractPathLiterals("patch(r, dyn, 1)")).toHaveLength(0);
  });
});
