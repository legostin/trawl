import { describe, it, expect } from "vitest";
import { cmpVersions } from "./plugins";

describe("cmpVersions", () => {
  it("detects a newer version", () => {
    expect(cmpVersions("0.1.1", "0.1.0")).toBeGreaterThan(0);
    expect(cmpVersions("1.0.0", "0.9.9")).toBeGreaterThan(0);
    expect(cmpVersions("0.2.0", "0.1.9")).toBeGreaterThan(0);
  });

  it("detects equal and older versions", () => {
    expect(cmpVersions("1.2.3", "1.2.3")).toBe(0);
    expect(cmpVersions("0.1.0", "0.1.1")).toBeLessThan(0);
  });

  it("handles differing segment counts", () => {
    expect(cmpVersions("1.0", "1.0.0")).toBe(0);
    expect(cmpVersions("1.0.1", "1.0")).toBeGreaterThan(0);
  });
});
