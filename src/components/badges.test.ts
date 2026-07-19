import { describe, it, expect } from "vitest";
import { methodColor, statusColor } from "./badges";

describe("methodColor", () => {
  it("maps common methods to colors", () => {
    expect(methodColor("GET")).toBe("text-http-green");
    expect(methodColor("post")).toBe("text-http-blue");
    expect(methodColor("PUT")).toBe("text-http-amber");
    expect(methodColor("PATCH")).toBe("text-http-amber");
    expect(methodColor("DELETE")).toBe("text-http-red");
    expect(methodColor("WEIRD")).toBe("text-http-gray");
  });
});

describe("statusColor", () => {
  it("maps status classes to colors", () => {
    expect(statusColor(200)).toBe("text-http-green");
    expect(statusColor(301)).toBe("text-http-blue");
    expect(statusColor(404)).toBe("text-http-amber");
    expect(statusColor(500)).toBe("text-http-red");
    expect(statusColor(undefined)).toBe("text-http-gray");
    expect(statusColor(100)).toBe("text-http-gray");
  });
});
