import { describe, it, expect } from "vitest";
import { API_DTS } from "./apiTypes";

describe("API_DTS", () => {
  it("documents ctx.breakpoint()", () => {
    expect(API_DTS).toContain("breakpoint(");
  });
});
